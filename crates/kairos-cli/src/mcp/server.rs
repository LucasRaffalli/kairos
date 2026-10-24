//! Le serveur MCP en stdio : `kairos mcp`. Il ne fait que relier `rmcp` au registre `tools` ;
//! la sortie standard ne porte que du protocole, tout le reste part sur la sortie d'erreur.

use std::sync::Arc;

use kairos_core::db::Db;
use kairos_core::service::Kairos;
use rmcp::model::{
    CallToolRequestParams, CallToolResponse, CallToolResult, ContentBlock, Implementation,
    ListToolsResult, PaginatedRequestParams, ServerCapabilities, ServerConfig, Tool,
};
use rmcp::service::RequestContext;
use rmcp::{ErrorData as McpError, RoleServer, ServerHandler, ServiceExt};
use serde_json::{json, Value};

use super::tools::{self, ProtocolError, ToolResult};
use crate::app_clock;

/// `order` fait attendre les appels les uns derrière les autres, dans l'ordre d'arrivée :
/// Claude en envoie souvent plusieurs d'un coup, et `start` doit passer avant son `stop`.
struct KairosServer {
    order: tokio::sync::Mutex<()>,
}

impl ServerHandler for KairosServer {
    fn get_info(&self) -> ServerConfig {
        let mut info = ServerConfig::new(ServerCapabilities::builder().enable_tools().build());
        info.server_info = Implementation::new("kairos", kairos_core::version());
        info.instructions = Some(
            "Kairos suit le temps passé sur les tâches Monday. Lance, change et arrête le \
             chronomètre ; ne devine jamais un ID Monday, demande-le."
                .into(),
        );
        info
    }

    async fn list_tools(
        &self,
        _request: Option<PaginatedRequestParams>,
        _context: RequestContext<RoleServer>,
    ) -> Result<ListToolsResult, McpError> {
        let tools = tools::definitions()
            .into_iter()
            .map(|d| {
                let schema = match d.schema {
                    Value::Object(map) => map,
                    _ => serde_json::Map::new(),
                };
                Tool::new(d.name, d.description, Arc::new(schema))
            })
            .collect();
        Ok(ListToolsResult::with_all_items(tools))
    }

    async fn call_tool(
        &self,
        request: CallToolRequestParams,
        context: RequestContext<RoleServer>,
    ) -> Result<CallToolResponse, McpError> {
        let _turn = self.order.lock().await;
        if context.ct.is_cancelled() {
            return Err(McpError::internal_error("requête annulée", None));
        }
        let client = context.client_info().map(|i| i.name);
        let source = tools::client_source(client.as_deref());
        let name = request.name.to_string();
        let args = request.arguments.map(Value::Object).unwrap_or(Value::Null);
        // La base est ouverte à chaque appel, dans un thread à part (SQLite bloque).
        let outcome = tokio::task::spawn_blocking(move || match Db::open(&Db::default_path()) {
            Ok(db) => tools::call(&Kairos::new(db, app_clock()), source, &name, args),
            Err(error) => Ok(ToolResult::Failed(
                json!({ "error": { "code": error.code(), "message": error.to_string() } }),
            )),
        })
        .await
        .map_err(|e| McpError::internal_error(format!("appel interrompu : {e}"), None))?;
        match outcome {
            Ok(ToolResult::Success(value)) => {
                Ok(CallToolResponse::Complete(CallToolResult::success(vec![
                    ContentBlock::text(value.to_string()),
                ])))
            }
            Ok(ToolResult::Failed(value)) => {
                Ok(CallToolResponse::Complete(CallToolResult::error(vec![
                    ContentBlock::text(value.to_string()),
                ])))
            }
            Err(ProtocolError::UnknownTool(name)) => Err(McpError::invalid_params(
                format!("outil inconnu : {name}"),
                None,
            )),
            Err(ProtocolError::InvalidParams(message)) => {
                Err(McpError::invalid_params(message, None))
            }
        }
    }
}

/// Sert jusqu'à la fermeture de l'entrée. Code de sortie : 0 en fin normale, 1 si le transport
/// ne démarre pas.
pub fn run() -> i32 {
    let runtime = match tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("kairos mcp : {e}");
            return 1;
        }
    };
    runtime.block_on(async {
        let server = KairosServer {
            order: tokio::sync::Mutex::new(()),
        };
        match server.serve(rmcp::transport::stdio()).await {
            Ok(service) => {
                if let Err(e) = service.waiting().await {
                    eprintln!("kairos mcp : {e}");
                }
                0
            }
            Err(e) => {
                eprintln!("kairos mcp : {e}");
                1
            }
        }
    })
}
