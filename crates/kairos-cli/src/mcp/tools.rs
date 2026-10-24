//! Le registre des 17 outils MCP : définitions, lecture des paramètres, appel du service,
//! règle d'autonomie. Aucune règle métier ici : tout passe par `Kairos`.

use kairos_core::clock::Clock;
use kairos_core::error::Error;
use kairos_core::model::{AddEntry, DayStatus, EditEntry, Source, StartArgs};
use kairos_core::service::Kairos;
use kairos_core::timeline::{self, format_duration};
use serde::de::DeserializeOwned;
use serde::Deserialize;
use serde_json::{json, Value};

use crate::render::{self, Outcome};
use crate::time_input;

/// Un outil tel que Claude le voit : un nom, une description qui dit **quand** s'en servir,
/// et le schéma JSON de ses paramètres.
pub struct ToolDef {
    pub name: &'static str,
    pub description: &'static str,
    pub schema: Value,
}

/// Le résultat d'un appel qui est arrivé jusqu'au service.
#[derive(Debug)]
pub enum ToolResult {
    /// Le JSON de `kairos --json` (ou une demande de confirmation).
    Success(Value),
    /// Une erreur métier : `{"error":{"code","message"}}`.
    Failed(Value),
}

/// Un appel mal formé : erreur de protocole, pas une erreur métier.
#[derive(Debug)]
pub enum ProtocolError {
    UnknownTool(String),
    InvalidParams(String),
}

enum Failure {
    Protocol(ProtocolError),
    Business(Error),
}

impl From<Error> for Failure {
    fn from(error: Error) -> Self {
        Failure::Business(error)
    }
}

/// La source des lots lancés par ce client : `claude-code` si son nom contient « code »,
/// sinon `claude-desktop` (nom absent compris).
pub fn client_source(client_name: Option<&str>) -> Source {
    if client_name.is_some_and(|n| n.to_lowercase().contains("code")) {
        Source::ClaudeCode
    } else {
        Source::ClaudeDesktop
    }
}

// --- Définitions ------------------------------------------------------------------------------

fn tool(
    name: &'static str,
    description: &'static str,
    properties: &[(&str, &str, &str)],
    required: &[&str],
) -> ToolDef {
    let props: serde_json::Map<String, Value> = properties
        .iter()
        .map(|(n, t, d)| ((*n).to_string(), json!({ "type": t, "description": d })))
        .collect();
    let mut schema =
        json!({ "type": "object", "properties": props, "additionalProperties": false });
    if !required.is_empty() {
        schema["required"] = json!(required);
    }
    ToolDef {
        name,
        description,
        schema,
    }
}

const TIME: &str = "Heure : 13:30 (locale), AAAA-MM-JJTHH:MM ou RFC 3339.";
const DATE: &str = "Jour AAAA-MM-JJ (aujourd'hui par défaut).";
const CONFIRM: &str = "true seulement après que l'utilisateur a dit oui à ce qui lui a été décrit.";

pub fn definitions() -> Vec<ToolDef> {
    let lot = [
        (
            "parent",
            "string",
            "ID Monday de la tâche parent (chiffres).",
        ),
        (
            "label",
            "string",
            "Nom affiché du parent, mémorisé avec son ID.",
        ),
        (
            "name",
            "string",
            "Nom de la tâche quand il n'y a pas d'ID Monday.",
        ),
        (
            "subtask",
            "string",
            "Sous-tâche (Front, Back…) ; par défaut le poste principal.",
        ),
        (
            "subtask_id",
            "string",
            "ID Monday de la sous-tâche, si l'utilisateur l'a donné.",
        ),
        ("at", "string", TIME),
    ];
    vec![
        tool("start", "Lance un lot de travail sur une tâche parent Monday (ID) ou un nom de tâche. Refusé si un lot tourne déjà : utilise switch.", &lot, &[]),
        tool("stop", "Arrête le lot en cours. Ne l'appelle pas en fin de réponse : seulement quand l'utilisateur arrête de travailler dessus.", &[("at", "string", TIME)], &[]),
        tool("pause", "Met le lot en cours en pause ; resume le reprend.", &[], &[]),
        tool("resume", "Reprend le lot mis en pause.", &[], &[]),
        tool("switch", "Change de tâche : arrête le lot en cours et lance le suivant à la même seconde. À utiliser quand l'utilisateur passe à un autre ticket.", &lot, &[]),
        tool("status", "Dit ce que fait Kairos maintenant : rien, un lot en cours (depuis quand, durée, total du jour) ou en pause.", &[], &[]),
        tool("today", "Liste les entrées d'une journée avec leurs numéros, leurs bornes et leur total.", &[("date", "string", DATE)], &[]),
        tool("report", "Prépare la liste à saisir dans Monday : lignes par parent et sous-tâche, temps sans ID détaillés, écart avec les bornes, texte prêt à copier.", &[("date", "string", DATE)], &[]),
        tool(
            "add_entry",
            "Ajoute un temps oublié, après coup. Refusé s'il chevauche une autre entrée, traverse minuit ou est dans le futur.",
            &[("parent", "string", "ID Monday du parent."), ("label", "string", "Nom affiché du parent."), ("name", "string", "Nom de la tâche sans ID Monday."), ("subtask", "string", "Sous-tâche."), ("from", "string", TIME), ("to", "string", TIME), ("date", "string", "Jour de référence des heures HH:MM.")],
            &["from", "to"],
        ),
        tool(
            "edit_entry",
            "Corrige une entrée (heures, parent, nom ou sous-tâche), avec les mêmes règles qu'à l'ajout. Les numéros viennent de today.",
            &[("id", "integer", "Numéro de l'entrée."), ("from", "string", TIME), ("to", "string", TIME), ("parent", "string", "Nouvel ID Monday du parent."), ("name", "string", "Nouveau nom de tâche."), ("subtask", "string", "Nouvelle sous-tâche."), ("date", "string", "Jour de référence des heures HH:MM.")],
            &["id"],
        ),
        tool(
            "delete_entry",
            "Supprime une entrée. Sans l'accord de l'utilisateur, l'outil renvoie needs_confirmation avec ce qui serait supprimé : demande-lui, puis rappelle avec confirm: true.",
            &[("id", "integer", "Numéro de l'entrée."), ("confirm", "boolean", CONFIRM)],
            &["id"],
        ),
        tool(
            "close_day",
            "Clôture une journée et fige ses totaux. Sans l'accord de l'utilisateur, l'outil renvoie needs_confirmation : demande-lui, puis rappelle avec confirm: true. Refusé si un lot tourne, sauf stop: true.",
            &[("date", "string", DATE), ("stop", "boolean", "Arrête d'abord le lot en cours."), ("confirm", "boolean", CONFIRM)],
            &[],
        ),
        tool(
            "mark_declared",
            "Note que l'utilisateur a saisi la journée dans Monday (quand il dit « c'est saisi »). Sans son accord, l'outil renvoie needs_confirmation : rappelle avec confirm: true après son oui.",
            &[("date", "string", DATE), ("confirm", "boolean", CONFIRM)],
            &[],
        ),
        tool(
            "recent",
            "Les lots des 7 derniers jours (nom, ID Monday, temps du jour, lot en cours). À appeler quand l'utilisateur parle d'une tâche sans donner d'ID (« retourne sur Bundle », « on reprend ») pour retrouver le bon lot, puis appelle switch avec son ID ; plusieurs candidats ou aucun : demande.",
            &[("limit", "integer", "Nombre de lots au plus (8 par défaut).")],
            &[],
        ),
        tool(
            "back",
            "Revient au lot d'avant (arrête le lot actuel, relance le précédent). Deux appels de suite font l'aller-retour. À utiliser quand l'utilisateur veut reprendre ce qu'il faisait juste avant, sans nommer la tâche.",
            &[],
            &[],
        ),
        tool(
            "set_label",
            "Enregistre le nom d'une tâche parent dont tu connais l'ID (« Bundle », « Refonte »), pour la retrouver avec recent. À faire quand un lot a été lancé sans nom.",
            &[("parent", "string", "ID Monday du parent."), ("label", "string", "Nom court de la tâche.")],
            &["parent", "label"],
        ),
        tool(
            "set_subtask_id",
            "Mémorise l'ID Monday d'une sous-tâche d'un parent, quand l'utilisateur le donne.",
            &[("parent", "string", "ID Monday du parent."), ("subtask", "string", "Nom de la sous-tâche."), ("id", "string", "ID Monday de la sous-tâche (chiffres).")],
            &["parent", "subtask", "id"],
        ),
    ]
}

// --- Paramètres -------------------------------------------------------------------------------

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct NoParams {}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StartParams {
    parent: Option<String>,
    label: Option<String>,
    name: Option<String>,
    subtask: Option<String>,
    subtask_id: Option<String>,
    at: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StopParams {
    at: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DateParams {
    date: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct AddEntryParams {
    parent: Option<String>,
    label: Option<String>,
    name: Option<String>,
    subtask: Option<String>,
    from: String,
    to: String,
    date: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct EditEntryParams {
    id: i64,
    from: Option<String>,
    to: Option<String>,
    parent: Option<String>,
    name: Option<String>,
    subtask: Option<String>,
    date: Option<String>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DeleteEntryParams {
    id: i64,
    confirm: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct CloseDayParams {
    date: Option<String>,
    stop: Option<bool>,
    confirm: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct MarkDeclaredParams {
    date: Option<String>,
    confirm: Option<bool>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct RecentParams {
    limit: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetLabelParams {
    parent: String,
    label: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct SetSubtaskIdParams {
    parent: String,
    subtask: String,
    id: String,
}

fn parse<T: DeserializeOwned>(args: Value) -> std::result::Result<T, Failure> {
    serde_json::from_value(args)
        .map_err(|e| Failure::Protocol(ProtocolError::InvalidParams(e.to_string())))
}

// --- Appel ------------------------------------------------------------------------------------

/// Exécute un outil. Les erreurs métier deviennent un `ToolResult::Failed` ; seul un appel mal
/// formé (outil inconnu, paramètre absent, inconnu ou du mauvais type) est une `ProtocolError`.
pub fn call<C: Clock>(
    k: &Kairos<C>,
    source: Source,
    name: &str,
    args: Value,
) -> std::result::Result<ToolResult, ProtocolError> {
    let args = if args.is_null() { json!({}) } else { args };
    match dispatch(k, source, name, args) {
        Ok(value) => Ok(ToolResult::Success(value)),
        Err(Failure::Business(error)) => Ok(ToolResult::Failed(
            json!({ "error": { "code": error.code(), "message": error.to_string() } }),
        )),
        Err(Failure::Protocol(error)) => Err(error),
    }
}

fn dispatch<C: Clock>(
    k: &Kairos<C>,
    source: Source,
    name: &str,
    args: Value,
) -> std::result::Result<Value, Failure> {
    let offset = |at: i64| k.clock().utc_offset(at);
    let today = || timeline::local_date(k.clock().now(), &offset);
    let time = |input: &str, date: &str| time_input::parse(input, date, &offset);
    let optional_time =
        |input: &Option<String>, date: &str| input.as_deref().map(|t| time(t, date)).transpose();
    let reference = |date: &Option<String>| date.clone().unwrap_or_else(today);

    let outcome = match name {
        "start" | "switch" => {
            let p: StartParams = parse(args)?;
            let start = StartArgs {
                at: optional_time(&p.at, &today())?,
                parent_monday_id: p.parent,
                parent_label: p.label,
                name: p.name,
                subtask: p.subtask,
                subtask_monday_id: p.subtask_id,
                source,
            };
            if name == "start" {
                Outcome::Started(k.start(start)?)
            } else {
                Outcome::Switched(k.switch_to(start)?)
            }
        }
        "stop" => {
            let p: StopParams = parse(args)?;
            Outcome::Stopped(k.stop(optional_time(&p.at, &today())?)?)
        }
        "pause" => {
            let _: NoParams = parse(args)?;
            Outcome::Paused(k.pause()?)
        }
        "resume" => {
            let _: NoParams = parse(args)?;
            Outcome::Resumed(k.resume()?)
        }
        "status" => {
            let _: NoParams = parse(args)?;
            Outcome::Status(k.status()?)
        }
        "today" => {
            let p: DateParams = parse(args)?;
            Outcome::Today(k.today(p.date.as_deref())?)
        }
        "report" => {
            let p: DateParams = parse(args)?;
            Outcome::Report(k.report(p.date.as_deref())?)
        }
        "add_entry" => {
            let p: AddEntryParams = parse(args)?;
            let day = reference(&p.date);
            Outcome::EntryAdded(k.add_entry(AddEntry {
                parent_monday_id: p.parent,
                parent_label: p.label,
                name: p.name,
                subtask: p.subtask,
                from: time(&p.from, &day)?,
                to: time(&p.to, &day)?,
            })?)
        }
        "edit_entry" => {
            let p: EditEntryParams = parse(args)?;
            let day = reference(&p.date);
            let edit = EditEntry {
                from: optional_time(&p.from, &day)?,
                to: optional_time(&p.to, &day)?,
                parent_monday_id: p.parent,
                name: p.name,
                subtask: p.subtask,
            };
            Outcome::EntryEdited(k.edit_entry(p.id, edit)?)
        }
        "recent" => {
            let p: RecentParams = parse(args)?;
            Outcome::Recent(k.recent_lots(p.limit.unwrap_or(8))?)
        }
        "back" => {
            let _: NoParams = parse(args)?;
            Outcome::Switched(k.back(source)?)
        }
        "set_label" => {
            let p: SetLabelParams = parse(args)?;
            k.set_label(&p.parent, &p.label)?;
            Outcome::Label {
                parent: p.parent,
                label: p.label,
            }
        }
        "set_subtask_id" => {
            let p: SetSubtaskIdParams = parse(args)?;
            k.set_subtask_id(&p.parent, &p.subtask, &p.id)?;
            Outcome::SubtaskId {
                parent: p.parent,
                subtask: p.subtask,
            }
        }
        "delete_entry" => {
            let p: DeleteEntryParams = parse(args)?;
            if must_ask(k, p.confirm)? {
                let entry = k.entry_view(p.id)?;
                let end = entry.end.map_or("en cours".to_string(), |end| hm(k, end));
                let would = format!(
                    "supprimer l'entrée #{} ({}, {} → {})",
                    entry.id,
                    entry.lot.describe(),
                    hm(k, entry.start),
                    end
                );
                return Ok(needs_confirmation(name, would));
            }
            k.delete_entry(p.id)?;
            Outcome::EntryDeleted(p.id)
        }
        "close_day" => {
            let p: CloseDayParams = parse(args)?;
            let stop = p.stop.unwrap_or(false);
            if must_ask(k, p.confirm)? {
                return Ok(needs_confirmation(
                    name,
                    close_day_sentence(k, p.date.as_deref(), stop)?,
                ));
            }
            Outcome::Closed(k.close_day(p.date.as_deref(), stop)?)
        }
        "mark_declared" => {
            let p: MarkDeclaredParams = parse(args)?;
            if must_ask(k, p.confirm)? {
                let day = k.today(p.date.as_deref())?;
                if day.status == DayStatus::Open {
                    return Err(Error::DayNotClosed(day.date).into());
                }
                if day.status != DayStatus::Declared {
                    return Ok(needs_confirmation(
                        name,
                        format!("marquer la journée {} saisie dans Monday", day.date),
                    ));
                }
            }
            Outcome::Declared(k.mark_declared(p.date.as_deref())?)
        }
        other => {
            return Err(Failure::Protocol(ProtocolError::UnknownTool(
                other.to_string(),
            )))
        }
    };
    Ok(render::to_json(&outcome))
}

// --- Autonomie --------------------------------------------------------------------------------

/// Faut-il demander avant d'agir ? Oui sauf si l'utilisateur a confirmé (`confirm: true`) ou si le
/// réglage `claude_autonomy` vaut `auto`. Le réglage est relu à **chaque** appel.
fn must_ask<C: Clock>(k: &Kairos<C>, confirm: Option<bool>) -> std::result::Result<bool, Error> {
    if confirm == Some(true) {
        return Ok(false);
    }
    Ok(k.config_get("claude_autonomy")? != "auto")
}

fn needs_confirmation(action: &str, would: String) -> Value {
    json!({
        "needs_confirmation": true,
        "action": action,
        "would": would,
        "how": "Demande à l'utilisateur, puis rappelle l'outil avec confirm: true."
    })
}

/// « clôturer la journée 2026-10-23 (1 h 05, 2 lignes) » — après avoir vérifié que la clôture est possible :
/// on ne demande jamais de confirmer une action qui échouerait.
fn close_day_sentence<C: Clock>(
    k: &Kairos<C>,
    date: Option<&str>,
    stop: bool,
) -> std::result::Result<String, Error> {
    let day = k.today(date)?;
    if day.status != DayStatus::Open {
        return Err(Error::DayNotOpen(day.date));
    }
    let live: Vec<_> = day.entries.iter().filter(|e| !e.removed).collect();
    if live.is_empty() {
        return Err(Error::DayEmpty(day.date));
    }
    let running = live.iter().find(|e| e.end.is_none());
    if let (Some(entry), false) = (running, stop) {
        return Err(Error::RunningLotOnClose(entry.lot.describe()));
    }
    let report = k.report(date)?;
    let lines = report.lines.iter().map(|l| l.subtasks.len()).sum::<usize>()
        + usize::from(!report.no_id.is_empty());
    let plural = if lines == 1 { "ligne" } else { "lignes" };
    let stopped = if running.is_some() {
        ", le lot en cours sera arrêté"
    } else {
        ""
    };
    Ok(format!(
        "clôturer la journée {} ({}, {lines} {plural}{stopped})",
        day.date,
        format_duration(report.total)
    ))
}

fn hm<C: Clock>(k: &Kairos<C>, at: i64) -> String {
    let secs = (at + i64::from(k.clock().utc_offset(at))).rem_euclid(86_400);
    format!("{} h {:02}", secs / 3600, (secs % 3600) / 60)
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_core::clock::FakeClock;
    use kairos_core::db::Db;
    use kairos_core::model::Source;
    use kairos_core::service::Kairos;
    use serde_json::{json, Value};

    fn at(h: u32, m: u32) -> i64 {
        chrono::NaiveDate::from_ymd_opt(2026, 10, 23)
            .unwrap()
            .and_hms_opt(h, m, 0)
            .unwrap()
            .and_utc()
            .timestamp()
    }

    /// Un service sur une base temporaire, à 17 h 00 le 23 octobre 2026 (heure locale = UTC).
    fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
        let dir = tempfile::tempdir().unwrap();
        let db = Db::open(&dir.path().join("kairos.db")).unwrap();
        (Kairos::new(db, FakeClock::new(at(17, 0), 0)), dir)
    }

    fn run(k: &Kairos<FakeClock>, name: &str, args: Value) -> Result<ToolResult, ProtocolError> {
        call(k, Source::ClaudeCode, name, args)
    }

    fn success(k: &Kairos<FakeClock>, name: &str, args: Value) -> Value {
        match run(k, name, args).unwrap() {
            ToolResult::Success(v) => v,
            ToolResult::Failed(v) => panic!("attendu un succès, reçu l'erreur {v}"),
        }
    }

    fn failed(k: &Kairos<FakeClock>, name: &str, args: Value) -> Value {
        match run(k, name, args).unwrap() {
            ToolResult::Failed(v) => v,
            ToolResult::Success(v) => panic!("attendu une erreur métier, reçu {v}"),
        }
    }

    fn protocol_error(k: &Kairos<FakeClock>, name: &str, args: Value) -> ProtocolError {
        run(k, name, args).unwrap_err()
    }

    fn entries(k: &Kairos<FakeClock>) -> i64 {
        k.db()
            .conn()
            .query_row("SELECT COUNT(*) FROM entries", [], |r| r.get(0))
            .unwrap()
    }

    const EXPECTED: [&str; 17] = [
        "add_entry",
        "back",
        "close_day",
        "delete_entry",
        "edit_entry",
        "mark_declared",
        "pause",
        "recent",
        "report",
        "resume",
        "set_label",
        "set_subtask_id",
        "start",
        "status",
        "stop",
        "switch",
        "today",
    ];

    #[test]
    fn definitions_are_exactly_the_seventeen_tools() {
        let mut names: Vec<_> = definitions().iter().map(|d| d.name).collect();
        names.sort_unstable();
        assert_eq!(names, EXPECTED);
    }

    #[test]
    fn recent_back_and_set_label_work() {
        let (k, _d) = kairos();
        success(
            &k,
            "start",
            json!({"parent": "12406850311", "label": "Bundle"}),
        );
        k.clock().advance(600);
        success(&k, "switch", json!({"parent": "12406856533"}));
        k.clock().advance(600);
        let recent = success(&k, "recent", json!({}));
        assert_eq!(recent.as_array().unwrap().len(), 2);
        assert_eq!(recent[0]["current"], true);
        assert_eq!(recent[0]["lot"]["parent_monday_id"], "12406856533");
        assert_eq!(recent[1]["lot"]["parent_label"], "Bundle");
        let back = success(&k, "back", json!({}));
        assert_eq!(back["started"]["lot"]["parent_monday_id"], "12406850311");
        success(
            &k,
            "set_label",
            json!({"parent": "12406850311", "label": "Bundle v2"}),
        );
        let limited = success(&k, "recent", json!({"limit": 1}));
        assert_eq!(limited.as_array().unwrap().len(), 1);
        assert_eq!(limited[0]["lot"]["parent_label"], "Bundle v2");
        assert_eq!(
            failed(&k, "set_label", json!({"parent": "1", "label": "x"}))["error"]["code"],
            "parent_not_found"
        );
        let (fresh, _d2) = kairos();
        assert_eq!(
            failed(&fresh, "back", json!({}))["error"]["code"],
            "no_previous_lot"
        );
    }

    #[test]
    fn the_new_tools_reject_bad_parameters() {
        let (k, _d) = kairos();
        for (name, args) in [
            ("recent", json!({"limit": "3"})),
            ("recent", json!({"limit": -1})),
            ("recent", json!({"extra": 1})),
            ("back", json!({"extra": 1})),
            ("set_label", json!({"parent": "1"})),
            ("set_label", json!({"label": "x"})),
        ] {
            assert!(
                matches!(
                    protocol_error(&k, name, args.clone()),
                    ProtocolError::InvalidParams(_)
                ),
                "{name} {args}"
            );
        }
    }

    #[test]
    fn every_tool_has_a_description_and_an_object_schema() {
        for d in definitions() {
            assert!(
                !d.description.trim().is_empty() && d.description.len() < 500,
                "{}",
                d.name
            );
            assert_eq!(d.schema["type"], "object", "{}", d.name);
            assert!(d.schema["properties"].is_object(), "{}", d.name);
        }
        let delete = definitions()
            .into_iter()
            .find(|d| d.name == "delete_entry")
            .unwrap();
        assert_eq!(delete.schema["required"], json!(["id"]));
        assert!(
            delete.description.contains("confirm"),
            "la règle de confirmation est dans la description"
        );
    }

    #[test]
    fn no_tool_touches_settings() {
        for d in definitions() {
            let text = format!("{} {}", d.name, d.schema).to_lowercase();
            assert!(
                !text.contains("config") && !text.contains("setting") && !text.contains("autonomy"),
                "{}",
                d.name
            );
        }
    }

    #[test]
    fn week_and_unknown_tools_are_protocol_errors() {
        let (k, _d) = kairos();
        assert!(
            matches!(protocol_error(&k, "week", json!({})), ProtocolError::UnknownTool(n) if n == "week")
        );
        assert!(matches!(
            protocol_error(&k, "config", json!({})),
            ProtocolError::UnknownTool(_)
        ));
    }

    #[test]
    fn a_missing_required_param_is_a_protocol_error_naming_the_field() {
        let (k, _d) = kairos();
        match protocol_error(&k, "delete_entry", json!({})) {
            ProtocolError::InvalidParams(why) => assert!(why.contains("id"), "{why}"),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn wrong_types_are_protocol_errors_and_confirm_must_be_a_real_boolean() {
        let (k, _d) = kairos();
        for args in [
            json!({"id": "3"}),
            json!({"id": 3, "confirm": "true"}),
            json!({"id": 3, "confirm": 1}),
            json!({"id": 3, "confirm": "yes"}),
            json!({"id": 3.5}),
            json!({"id": null}),
        ] {
            assert!(
                matches!(
                    protocol_error(&k, "delete_entry", args.clone()),
                    ProtocolError::InvalidParams(_)
                ),
                "{args}"
            );
        }
        assert!(matches!(
            protocol_error(&k, "start", json!({"parent": 123})),
            ProtocolError::InvalidParams(_)
        ));
    }

    #[test]
    fn an_unknown_param_is_a_protocol_error() {
        let (k, _d) = kairos();
        assert!(matches!(
            protocol_error(&k, "status", json!({"foo": 1})),
            ProtocolError::InvalidParams(_)
        ));
        assert!(matches!(
            protocol_error(&k, "start", json!({"parent": "1", "source": "manual"})),
            ProtocolError::InvalidParams(_)
        ));
    }

    #[test]
    fn a_missing_arguments_object_is_the_same_as_an_empty_one() {
        let (k, _d) = kairos();
        assert_eq!(success(&k, "status", Value::Null)["state"], "idle");
    }

    #[test]
    fn start_status_pause_resume_switch_stop_roundtrip() {
        let (k, _d) = kairos();
        let started = success(&k, "start", json!({"parent": "12406850311"}));
        assert_eq!(
            (
                started["lot"]["subtask"].as_str(),
                started["entry"]["start"].as_i64()
            ),
            (Some("Front"), Some(at(17, 0)))
        );
        k.clock().advance(1800);
        let status = success(&k, "status", json!({}));
        assert_eq!(
            (status["state"].as_str(), status["duration"].as_i64()),
            (Some("running"), Some(1800))
        );
        assert_eq!(success(&k, "pause", json!({}))["duration"], 1800);
        k.clock().advance(60);
        assert_eq!(
            success(&k, "resume", json!({}))["lot"]["parent_monday_id"],
            "12406850311"
        );
        k.clock().advance(600);
        let switched = success(
            &k,
            "switch",
            json!({"parent": "12406856533", "subtask": "Back"}),
        );
        assert_eq!(switched["stopped"]["duration"], 600);
        assert_eq!(switched["started"]["lot"]["subtask"], "Back");
        k.clock().advance(300);
        assert_eq!(success(&k, "stop", json!({}))["duration"], 300);
        assert_eq!(success(&k, "status", json!({}))["state"], "idle");
    }

    #[test]
    fn a_business_error_is_failed_with_the_code_and_the_french_message() {
        let (k, _d) = kairos();
        success(&k, "start", json!({"parent": "1"}));
        let error = failed(&k, "start", json!({"parent": "2"}));
        assert_eq!(error["error"]["code"], "already_running");
        assert!(error["error"]["message"]
            .as_str()
            .unwrap()
            .contains("déjà en cours"));
        assert_eq!(
            failed(&k, "start", json!({"name": "a\nb"}))["error"]["code"],
            "invalid_name"
        );
        assert_eq!(
            failed(&k, "today", json!({"date": "hier"}))["error"]["code"],
            "invalid_date"
        );
        assert_eq!(
            failed(&k, "stop", json!({"at": "25:61"}))["error"]["code"],
            "invalid_time"
        );
    }

    #[test]
    fn the_source_follows_the_client_name() {
        assert_eq!(client_source(Some("claude-code")), Source::ClaudeCode);
        assert_eq!(client_source(Some("Claude Code")), Source::ClaudeCode);
        assert_eq!(client_source(Some("claude-desktop")), Source::ClaudeDesktop);
        assert_eq!(client_source(Some("Claude")), Source::ClaudeDesktop);
        assert_eq!(client_source(Some("")), Source::ClaudeDesktop);
        assert_eq!(client_source(None), Source::ClaudeDesktop);
        assert_eq!(client_source(Some("cursor")), Source::ClaudeDesktop);
    }

    #[test]
    fn lots_started_through_the_tools_carry_the_source() {
        let (k, _d) = kairos();
        call(&k, Source::ClaudeDesktop, "start", json!({"parent": "1"})).unwrap();
        let source: String = k
            .db()
            .conn()
            .query_row("SELECT source FROM lots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(source, "claude-desktop");
    }

    #[test]
    fn report_returns_the_text_and_the_no_id_details() {
        let (k, _d) = kairos();
        success(
            &k,
            "add_entry",
            json!({"parent": "12406850311", "subtask": "Front", "from": "09:00", "to": "10:00"}),
        );
        success(
            &k,
            "add_entry",
            json!({"name": "Réunion", "from": "10:00", "to": "10:15"}),
        );
        let report = success(&k, "report", json!({}));
        assert_eq!(
            report["text"],
            "12406850311 : Front 1 h 00 · sans ID : 0 h 15"
        );
        assert_eq!(report["no_id"][0]["entries"][0]["secs"], 900);
        assert_eq!(
            success(&k, "today", json!({}))["entries"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    }

    fn day_with_two_lines(k: &Kairos<FakeClock>) {
        success(
            k,
            "add_entry",
            json!({"parent": "12406850311", "subtask": "Front", "from": "09:00", "to": "10:00"}),
        );
        success(
            k,
            "add_entry",
            json!({"name": "Réunion", "from": "10:00", "to": "10:05"}),
        );
    }

    #[test]
    fn ask_mode_asks_before_delete_close_and_declare_and_changes_nothing() {
        let (k, _d) = kairos();
        day_with_two_lines(&k);
        let delete = success(&k, "delete_entry", json!({"id": 1}));
        assert_eq!(delete["needs_confirmation"], true);
        assert_eq!(delete["action"], "delete_entry");
        assert_eq!(
            delete["would"],
            "supprimer l'entrée #1 (Front sur 12406850311, 9 h 00 → 10 h 00)"
        );
        assert!(delete["how"].as_str().unwrap().contains("confirm: true"));
        let close = success(&k, "close_day", json!({}));
        assert_eq!(
            (
                close["needs_confirmation"].as_bool(),
                close["would"].as_str()
            ),
            (
                Some(true),
                Some("clôturer la journée 2026-10-23 (1 h 05, 2 lignes)")
            )
        );
        assert_eq!(entries(&k), 2);
        assert_eq!(success(&k, "today", json!({}))["status"], "open");
        // La clôture avec confirm, puis la saisie : même règle.
        success(&k, "close_day", json!({"confirm": true}));
        let declare = success(&k, "mark_declared", json!({}));
        assert_eq!(
            (
                declare["needs_confirmation"].as_bool(),
                declare["would"].as_str()
            ),
            (
                Some(true),
                Some("marquer la journée 2026-10-23 saisie dans Monday")
            )
        );
        assert_eq!(success(&k, "today", json!({}))["status"], "closed");
        // Journée déjà saisie : rien à confirmer, le cœur ne fait rien (idempotent).
        success(&k, "mark_declared", json!({"confirm": true}));
        let again = success(&k, "mark_declared", json!({}));
        assert_eq!(again["needs_confirmation"], Value::Null, "{again}");
        assert_eq!(again["status"], "declared");
    }

    #[test]
    fn confirm_true_executes_in_ask_mode_and_confirm_false_still_asks() {
        let (k, _d) = kairos();
        day_with_two_lines(&k);
        assert_eq!(
            success(&k, "delete_entry", json!({"id": 1, "confirm": false}))["needs_confirmation"],
            true
        );
        assert_eq!(entries(&k), 2);
        assert_eq!(
            success(&k, "delete_entry", json!({"id": 1, "confirm": true}))["deleted"],
            1
        );
        assert_eq!(entries(&k), 1);
    }

    #[test]
    fn auto_mode_executes_without_confirm_and_ignores_confirm_false() {
        let (k, _d) = kairos();
        day_with_two_lines(&k);
        k.config_set("claude_autonomy", "auto").unwrap();
        assert_eq!(
            success(&k, "delete_entry", json!({"id": 1, "confirm": false}))["deleted"],
            1
        );
        assert_eq!(success(&k, "close_day", json!({}))["status"], "closed");
        assert_eq!(
            success(&k, "mark_declared", json!({}))["status"],
            "declared"
        );
    }

    #[test]
    fn the_autonomy_setting_is_read_on_every_call() {
        let (k, _d) = kairos();
        day_with_two_lines(&k);
        assert_eq!(
            success(&k, "delete_entry", json!({"id": 1}))["needs_confirmation"],
            true
        );
        k.config_set("claude_autonomy", "auto").unwrap();
        assert_eq!(success(&k, "delete_entry", json!({"id": 1}))["deleted"], 1);
        k.config_set("claude_autonomy", "ask").unwrap();
        assert_eq!(
            success(&k, "delete_entry", json!({"id": 2}))["needs_confirmation"],
            true
        );
    }

    #[test]
    fn impossible_actions_are_errors_not_questions() {
        let (k, _d) = kairos();
        day_with_two_lines(&k);
        assert_eq!(
            failed(&k, "delete_entry", json!({"id": 99}))["error"]["code"],
            "entry_not_found"
        );
        assert_eq!(
            failed(&k, "close_day", json!({"date": "2026-10-01"}))["error"]["code"],
            "day_empty"
        );
        assert_eq!(
            failed(&k, "mark_declared", json!({}))["error"]["code"],
            "day_not_closed"
        );
        success(&k, "close_day", json!({"confirm": true}));
        assert_eq!(
            failed(&k, "close_day", json!({}))["error"]["code"],
            "day_not_open"
        );
    }

    #[test]
    fn closing_while_a_lot_runs_asks_nothing_until_stop_is_given() {
        let (k, _d) = kairos();
        success(&k, "start", json!({"parent": "1", "at": "16:00"}));
        assert_eq!(
            failed(&k, "close_day", json!({}))["error"]["code"],
            "running_lot_on_close"
        );
        let with_stop = success(&k, "close_day", json!({"stop": true}));
        assert_eq!(with_stop["needs_confirmation"], true);
        assert_eq!(
            success(&k, "status", json!({}))["state"],
            "running",
            "demander n'a rien arrêté"
        );
        assert_eq!(
            success(&k, "close_day", json!({"stop": true, "confirm": true}))["status"],
            "closed"
        );
    }

    #[test]
    fn the_other_tools_never_ask() {
        let (k, _d) = kairos();
        success(
            &k,
            "add_entry",
            json!({"parent": "1", "from": "09:00", "to": "10:00"}),
        );
        assert_eq!(
            success(&k, "edit_entry", json!({"id": 1, "to": "10:30"}))["end"],
            at(10, 30)
        );
        success(
            &k,
            "set_subtask_id",
            json!({"parent": "1", "subtask": "Front", "id": "12406850412"}),
        );
        success(&k, "start", json!({"parent": "1", "at": "16:00"}));
        assert!(success(&k, "stop", json!({}))["duration"].as_i64().unwrap() > 0);
    }

    #[test]
    fn a_task_name_of_500_characters_is_a_business_error() {
        let (k, _d) = kairos();
        let long = "é".repeat(500);
        assert_eq!(
            failed(&k, "start", json!({"name": long}))["error"]["code"],
            "invalid_name"
        );
    }
}
