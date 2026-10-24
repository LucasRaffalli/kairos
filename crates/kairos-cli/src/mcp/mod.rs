//! Le serveur MCP : les outils de Kairos pour Claude Code et Claude Desktop.
//!
//! `tools` est le registre des outils, sans dépendance à `rmcp` : définitions, lecture des
//! paramètres, appel du service, règle d'autonomie. Il ne contient aucune règle métier.

pub mod server;
pub mod tools;
