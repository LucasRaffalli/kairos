//! Le pont entre l'interface et le cœur : des fonctions fines sur `Kairos`, sans fenêtre
//! (donc testables), et les commandes Tauri qui les enveloppent. Aucune règle métier ici :
//! les refus du cœur sortent tels quels, `{ code, message }`, le message en français.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use kairos_core::clock::{Clock, SystemClock};
use kairos_core::db::Db;
use kairos_core::error::Error;
use kairos_core::model::{
    DayView, Overview, RecentLot, Source, StartArgs, StartedLot, Status, StoppedEntry,
    SubtaskChoices, SwitchResult,
};
use kairos_core::service::Kairos;

use crate::inflight::InFlight;

/// Attente maximale d'un verrou : au-delà, l'interface affiche l'erreur et « Réessayer ».
pub const OPEN_TIMEOUT: Duration = Duration::from_secs(2);

/// Nombre de lots récents proposés (« Récents » et « Reprendre »).
const RECENT_LIMIT: usize = 8;

/// Un refus du cœur, tel que l'interface le reçoit.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BridgeError {
    pub code: String,
    pub message: String,
}

impl From<Error> for BridgeError {
    fn from(error: Error) -> Self {
        BridgeError {
            code: error.code().to_string(),
            message: error.to_string(),
        }
    }
}

pub type BridgeResult<T> = Result<T, BridgeError>;

/// Tout ce qu'il faut pour dessiner le popover, en un appel : pas de scintillement.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Snapshot {
    pub status: Status,
    pub today: DayView,
    pub recent: Vec<RecentLot>,
    /// Les sous-tâches du lot en cours (ou en pause), sinon celles du poste principal.
    pub choices: SubtaskChoices,
    /// L'heure du cœur, en secondes : l'interface corrige l'écart avec l'horloge du navigateur.
    pub server_time: i64,
    /// Le dossier personnel, pour abréger les dossiers de travail en « ~/… ».
    pub home: String,
}

/// Bornes de `at` : 1970-01-01 et 2100-01-01 (UTC). Hors de cette plage le cœur ne sait plus
/// calculer le jour local ; les bornes le font répondre `too_old` ou `in_future`.
const MIN_AT: i64 = 0;
const MAX_AT: i64 = 4_102_444_800;

/// Un lancement demandé par l'interface (aussi pour `switch`). `at` : secondes, calculées par
/// l'interface depuis « HH:MM ».
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(default)]
pub struct StartRequest {
    pub parent_monday_id: Option<String>,
    pub parent_label: Option<String>,
    pub name: Option<String>,
    pub subtask: Option<String>,
    pub subtask_monday_id: Option<String>,
    pub at: Option<i64>,
}

impl From<StartRequest> for StartArgs {
    /// Un lancement depuis l'app est manuel et n'a pas de dossier de travail.
    fn from(request: StartRequest) -> Self {
        StartArgs {
            parent_monday_id: request.parent_monday_id,
            parent_label: request.parent_label,
            name: request.name,
            subtask: request.subtask,
            subtask_monday_id: request.subtask_monday_id,
            at: request.at.map(|at| at.clamp(MIN_AT, MAX_AT)),
            source: Source::Manual,
            workdir: None,
        }
    }
}

/// Ouvre la base pour un seul appel, à l'heure du système.
pub fn open() -> BridgeResult<Kairos<SystemClock>> {
    let db = Db::open_with_timeout(&Db::default_path(), OPEN_TIMEOUT)?;
    Ok(Kairos::new(db, SystemClock))
}

/// Tout l'état en une lecture du cœur (une transaction), et l'heure du cœur.
pub fn snapshot<C: Clock>(k: &Kairos<C>, home: &str) -> BridgeResult<Snapshot> {
    let Overview {
        status,
        today,
        recent,
        choices,
    } = k.overview(RECENT_LIMIT, None)?;
    Ok(Snapshot {
        status,
        today,
        recent,
        choices,
        server_time: k.clock().now(),
        home: home.to_string(),
    })
}

pub fn subtask_choices<C: Clock>(
    k: &Kairos<C>,
    parent: Option<String>,
) -> BridgeResult<SubtaskChoices> {
    Ok(k.subtask_choices(parent.as_deref())?)
}

pub fn start<C: Clock>(k: &Kairos<C>, request: StartRequest) -> BridgeResult<StartedLot> {
    Ok(k.start(request.into())?)
}

pub fn switch<C: Clock>(k: &Kairos<C>, request: StartRequest) -> BridgeResult<SwitchResult> {
    Ok(k.switch_to(request.into())?)
}

pub fn stop<C: Clock>(k: &Kairos<C>) -> BridgeResult<StoppedEntry> {
    Ok(k.stop(None)?)
}

pub fn pause<C: Clock>(k: &Kairos<C>) -> BridgeResult<StoppedEntry> {
    Ok(k.pause()?)
}

pub fn resume<C: Clock>(k: &Kairos<C>) -> BridgeResult<StartedLot> {
    Ok(k.resume()?)
}

/// ⌘K puis P : met en pause un lot qui tourne, reprend un lot en pause, ne fait rien au repos.
pub fn toggle_pause<C: Clock>(k: &Kairos<C>) -> BridgeResult<()> {
    match k.status()? {
        Status::Running { .. } => pause(k).map(|_| ()),
        Status::Paused { .. } => resume(k).map(|_| ()),
        Status::Idle => Ok(()),
    }
}

/// ⌘K puis ↵ : relance le dernier lot, comme « Reprendre » (sans formulaire). Rend `false`, sans
/// rien changer, si un lot tourne ou est en pause, ou s'il n'y a aucun lot récent.
pub fn relaunch_last<C: Clock>(k: &Kairos<C>) -> BridgeResult<bool> {
    if k.status()? != Status::Idle {
        return Ok(false);
    }
    let Some(recent) = k.recent_lots(1)?.into_iter().next() else {
        return Ok(false);
    };
    let lot = recent.lot;
    let has_parent = lot.parent_monday_id.is_some();
    let request = StartRequest {
        parent_label: lot.parent_label.filter(|_| has_parent),
        name: lot.name.filter(|_| !has_parent),
        parent_monday_id: lot.parent_monday_id,
        subtask: lot.subtask,
        subtask_monday_id: lot.subtask_monday_id,
        at: None,
    };
    start(k, request).map(|_| true)
}

/// Exécute une action sur la base hors des commandes Tauri (les raccourcis globaux) : comme
/// elles, elle compte dans `InFlight`, pour que quitter attende qu'elle soit finie.
pub fn perform<T>(
    actions: &InFlight,
    f: impl FnOnce(&Kairos<SystemClock>) -> BridgeResult<T>,
) -> BridgeResult<T> {
    let _guard = actions.begin();
    f(&open()?)
}

/// Un lot compte le temps : quitter demande alors quoi en faire. Une pause ne compte rien.
pub fn lot_is_running(status: &Status) -> bool {
    matches!(status, Status::Running { .. })
}

/// « Arrêter le lot et quitter » : arrête le lot s'il tourne ; rien à faire sinon.
pub fn stop_if_running<C: Clock>(k: &Kairos<C>) -> BridgeResult<()> {
    if lot_is_running(&k.status()?) {
        k.stop(None)?;
    }
    Ok(())
}

/// Le titre à côté de l'icône : « 0:42:13 » en cours, « Pause · 0:42:13 » en pause (la durée de
/// la dernière entrée), rien au repos. Avec `seconds` à faux (réglage `menu_title_seconds`), sans
/// secondes : « 0:42 », que le titre ne change alors qu'à la minute.
pub fn menu_title(status: &Status, now: i64, seconds: bool) -> Option<String> {
    match status {
        Status::Idle => None,
        Status::Running { since, .. } => Some(clock(now - since, seconds)),
        Status::Paused { duration, .. } => Some(format!("Pause · {}", clock(*duration, seconds))),
    }
}

/// Le réglage `menu_title_seconds` : seul « false » masque les secondes ; toute autre valeur
/// (invalide comprise) garde le défaut.
pub fn shows_seconds(setting: &str) -> bool {
    setting != "false"
}

/// Ce que le surveillant de l'icône lit dans la base : le statut et deux réglages internes.
pub struct MenuState {
    pub status: Status,
    /// `menu_title_seconds` : le titre porte les secondes.
    pub seconds: bool,
    /// `global_shortcuts` : ⌘K est actif.
    pub global_shortcuts: bool,
}

/// Le réglage `global_shortcuts` : seul « false » les désactive (une valeur invalide garde le défaut).
pub fn shortcuts_enabled(setting: &str) -> bool {
    setting != "false"
}

pub fn read_menu_state() -> BridgeResult<MenuState> {
    let k = open()?;
    Ok(MenuState {
        status: k.status()?,
        seconds: shows_seconds(&k.config_get("menu_title_seconds")?),
        global_shortcuts: shortcuts_enabled(&k.config_get("global_shortcuts")?),
    })
}

/// « H:MM:SS » ou, sans `seconds`, « H:MM » : heures sans zéro initial, le reste sur deux chiffres.
fn clock(duration: i64, seconds: bool) -> String {
    let duration = duration.max(0);
    let (hours, minutes) = (duration / 3600, duration / 60 % 60);
    if seconds {
        format!("{hours}:{minutes:02}:{:02}", duration % 60)
    } else {
        format!("{hours}:{minutes:02}")
    }
}

/// Les commandes Tauri : chacune ouvre la base, appelle la fonction du pont et rend son résultat.
/// La base est lue hors du fil principal, pour ne jamais figer l'interface pendant une attente
/// de verrou.
pub mod commands {
    use tauri::{AppHandle, State};

    use super::*;

    /// Ouvre la base et exécute `f` sur un fil de travail.
    async fn with_kairos<T, F>(f: F) -> BridgeResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Kairos<SystemClock>) -> BridgeResult<T> + Send + 'static,
    {
        tauri::async_runtime::spawn_blocking(move || f(&open()?))
            .await
            .map_err(|e| BridgeError::from(Error::Internal(e.to_string())))?
    }

    /// Comme [`with_kairos`], pour une écriture : quitter attend qu'elle soit finie.
    async fn with_action<T, F>(actions: &InFlight, f: F) -> BridgeResult<T>
    where
        T: Send + 'static,
        F: FnOnce(&Kairos<SystemClock>) -> BridgeResult<T> + Send + 'static,
    {
        let guard = actions.begin();
        with_kairos(move |k| {
            let _guard = guard;
            f(k)
        })
        .await
    }

    #[tauri::command]
    pub async fn snapshot() -> BridgeResult<Snapshot> {
        let home = std::env::var("HOME").unwrap_or_default();
        with_kairos(move |k| super::snapshot(k, &home)).await
    }

    #[tauri::command]
    pub async fn subtask_choices(parent: Option<String>) -> BridgeResult<SubtaskChoices> {
        with_kairos(move |k| super::subtask_choices(k, parent)).await
    }

    #[tauri::command]
    pub async fn start(
        actions: State<'_, InFlight>,
        request: StartRequest,
    ) -> BridgeResult<StartedLot> {
        with_action(&actions, move |k| super::start(k, request)).await
    }

    #[tauri::command]
    pub async fn switch(
        actions: State<'_, InFlight>,
        request: StartRequest,
    ) -> BridgeResult<SwitchResult> {
        with_action(&actions, move |k| super::switch(k, request)).await
    }

    #[tauri::command]
    pub async fn stop(actions: State<'_, InFlight>) -> BridgeResult<StoppedEntry> {
        with_action(&actions, super::stop).await
    }

    #[tauri::command]
    pub async fn pause(actions: State<'_, InFlight>) -> BridgeResult<StoppedEntry> {
        with_action(&actions, super::pause).await
    }

    #[tauri::command]
    pub async fn resume(actions: State<'_, InFlight>) -> BridgeResult<StartedLot> {
        with_action(&actions, super::resume).await
    }

    /// Quitte l'app, en arrêtant d'abord le lot si demandé, et une fois les actions en vol
    /// terminées. Si l'arrêt échoue, l'app reste ouverte et l'interface montre le refus : on ne
    /// perd jamais un lot en silence. L'argument s'écrit `stop_lot` côté interface, comme le
    /// JSON du cœur.
    #[tauri::command(rename_all = "snake_case")]
    pub async fn quit(
        app: AppHandle,
        actions: State<'_, InFlight>,
        stop_lot: bool,
    ) -> BridgeResult<()> {
        if stop_lot {
            with_action(&actions, stop_if_running).await?;
        }
        tauri::async_runtime::spawn_blocking(move || crate::tray::exit_when_idle(&app))
            .await
            .map_err(|e| BridgeError::from(Error::Internal(e.to_string())))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use kairos_core::clock::FakeClock;
    use kairos_core::db::Db;
    use kairos_core::model::LotView;

    /// Le vendredi 23 octobre 2026 à 9 h 00 UTC.
    const T9: i64 = 1_792_713_600 + 9 * 3600;

    /// Un service sur une base temporaire, à 9 h 00 le 23 octobre 2026 (heure locale = UTC).
    fn kairos() -> (Kairos<FakeClock>, tempfile::TempDir) {
        let dir = tempfile::tempdir().expect("dossier temporaire");
        let db = Db::open(&dir.path().join("kairos.db")).expect("base temporaire");
        (Kairos::new(db, FakeClock::new(T9, 0)), dir)
    }

    fn request(parent: &str) -> StartRequest {
        StartRequest {
            parent_monday_id: Some(parent.into()),
            subtask: Some("Front".into()),
            ..StartRequest::default()
        }
    }

    #[test]
    fn snapshot_of_an_empty_database_is_idle_with_main_subtask_choices() {
        let (k, _d) = kairos();
        let snap = snapshot(&k, "/Users/lucas").unwrap();
        assert_eq!(snap.status, Status::Idle);
        assert!(snap.today.entries.is_empty());
        assert!(snap.recent.is_empty());
        assert_eq!(snap.choices, k.subtask_choices(None).unwrap());
        assert_eq!(snap.choices.main, "Front");
        assert!(snap.choices.own.is_empty());
        assert_eq!(snap.server_time, T9);
        assert_eq!(snap.home, "/Users/lucas");
    }

    #[test]
    fn snapshot_with_a_running_lot_has_status_today_and_recent() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        k.clock().advance(42 * 60);
        let snap = snapshot(&k, "/Users/lucas").unwrap();
        match &snap.status {
            Status::Running {
                lot,
                since,
                duration,
                launched_by,
                workdir,
                ..
            } => {
                assert_eq!(lot.parent_monday_id.as_deref(), Some("12406850311"));
                assert_eq!((*since, *duration), (T9, 42 * 60));
                assert_eq!(*launched_by, Some(Source::Manual));
                assert_eq!(*workdir, None);
            }
            other => panic!("attendu Running, reçu {other:?}"),
        }
        assert_eq!(snap.today.entries.len(), 1);
        assert_eq!(snap.recent.len(), 1);
        assert!(snap.recent[0].current);
        assert_eq!(
            snap.choices,
            k.subtask_choices(Some("12406850311")).unwrap()
        );
        assert_eq!(snap.server_time, T9 + 42 * 60);
    }

    #[test]
    fn snapshot_serializes_with_the_core_field_names() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        let json = serde_json::to_value(snapshot(&k, "/Users/lucas").unwrap()).unwrap();
        assert_eq!(json["status"]["state"], "running");
        assert_eq!(json["status"]["launched_by"], "manual");
        for key in ["today", "recent", "choices", "server_time", "home"] {
            assert!(json.get(key).is_some(), "champ manquant : {key}");
        }
    }

    #[test]
    fn start_then_stop_through_the_bridge_match_the_core() {
        let (k, _d) = kairos();
        let started = start(&k, request("12406850311")).unwrap();
        assert_eq!(started.lot.subtask.as_deref(), Some("Front"));
        assert_eq!(started.entry.source, Some(Source::Manual));
        assert_eq!(started.entry.workdir, None);
        k.clock().advance(600);
        let stopped = stop(&k).unwrap();
        assert_eq!(stopped.duration, 600);
        assert_eq!(k.status().unwrap(), Status::Idle);
    }

    #[test]
    fn switch_pause_and_resume_go_through_the_core() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        k.clock().advance(300);
        let switched = switch(&k, request("98765")).unwrap();
        assert_eq!(switched.stopped.map(|s| s.duration), Some(300));
        assert_eq!(
            switched.started.lot.parent_monday_id.as_deref(),
            Some("98765")
        );
        k.clock().advance(120);
        assert_eq!(pause(&k).unwrap().duration, 120);
        assert!(matches!(k.status().unwrap(), Status::Paused { .. }));
        resume(&k).unwrap();
        assert!(matches!(k.status().unwrap(), Status::Running { .. }));
    }

    #[test]
    fn a_refusal_keeps_its_code_and_french_message() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        let refused = start(&k, request("98765")).unwrap_err();
        let core = k
            .start(StartArgs::from(request("98765")))
            .unwrap_err()
            .to_string();
        assert_eq!(
            refused,
            BridgeError {
                code: "already_running".into(),
                message: core,
            }
        );
        assert!(refused.message.starts_with("Un lot est déjà en cours"));
    }

    #[test]
    fn start_with_a_future_time_is_refused_as_in_future() {
        let (k, _d) = kairos();
        let refused = start(
            &k,
            StartRequest {
                at: Some(T9 + 3600),
                ..request("12406850311")
            },
        )
        .unwrap_err();
        assert_eq!(refused.code, "in_future");
        assert!(refused.message.contains("dans le futur"));
    }

    #[test]
    fn an_extreme_start_time_is_refused_by_the_core_instead_of_panicking() {
        let (k, _d) = kairos();
        let at = |seconds: i64| {
            start(
                &k,
                StartRequest {
                    at: Some(seconds),
                    ..request("12406850311")
                },
            )
            .unwrap_err()
            .code
        };
        assert_eq!(at(i64::MIN), "too_old");
        assert_eq!(at(i64::MAX), "in_future");
    }

    #[test]
    fn a_reasonable_start_time_is_not_altered() {
        let args = StartArgs::from(StartRequest {
            at: Some(T9 - 600),
            ..StartRequest::default()
        });
        assert_eq!(args.at, Some(T9 - 600));
    }

    #[test]
    fn subtask_choices_of_an_unknown_parent_have_no_own_subtasks() {
        let (k, _d) = kairos();
        let choices = subtask_choices(&k, Some("555".into())).unwrap();
        assert_eq!(choices.main, "Front");
        assert!(choices.own.is_empty());
        assert_eq!(
            subtask_choices(&k, Some("12a".into())).unwrap_err().code,
            "invalid_monday_id"
        );
    }

    #[test]
    fn the_request_reads_the_snake_case_json_of_the_interface() {
        let parsed: StartRequest = serde_json::from_str(
            r#"{"parent_monday_id":"12406850311","parent_label":"Refonte","subtask":"Back","subtask_monday_id":"42","at":1792746000}"#,
        )
        .unwrap();
        let args = StartArgs::from(parsed);
        assert_eq!(args.parent_monday_id.as_deref(), Some("12406850311"));
        assert_eq!(args.parent_label.as_deref(), Some("Refonte"));
        assert_eq!(args.name, None);
        assert_eq!(args.subtask.as_deref(), Some("Back"));
        assert_eq!(args.subtask_monday_id.as_deref(), Some("42"));
        assert_eq!(args.at, Some(1_792_746_000));
        assert_eq!(args.source, Source::Manual);
        assert_eq!(args.workdir, None);
        let empty: StartRequest = serde_json::from_str("{}").unwrap();
        assert_eq!(empty.parent_monday_id, None);
    }

    fn running(since: i64) -> Status {
        Status::Running {
            lot: lot(),
            since,
            duration: 0,
            today_total: 0,
            launched_by: None,
            workdir: None,
        }
    }

    fn lot() -> LotView {
        LotView {
            id: 1,
            parent_monday_id: Some("12406850311".into()),
            parent_label: None,
            subtask: Some("Front".into()),
            subtask_monday_id: None,
            name: None,
        }
    }

    #[test]
    fn menu_title_formats() {
        let at = |secs: i64| menu_title(&running(T9), T9 + secs, true);
        assert_eq!(at(0).as_deref(), Some("0:00:00"));
        assert_eq!(at(42).as_deref(), Some("0:00:42"));
        assert_eq!(at(59).as_deref(), Some("0:00:59"));
        assert_eq!(at(60).as_deref(), Some("0:01:00"));
        assert_eq!(at(42 * 60 + 13).as_deref(), Some("0:42:13"));
        assert_eq!(at(3600 + 5 * 60 + 9).as_deref(), Some("1:05:09"));
        assert_eq!(at(12 * 3600).as_deref(), Some("12:00:00"));
        // Une horloge en retard sur le début ne donne pas une durée négative.
        assert_eq!(at(-30).as_deref(), Some("0:00:00"));
        let paused = Status::Paused {
            lot: lot(),
            since: T9,
            duration: 42 * 60 + 13,
            launched_by: None,
            workdir: None,
        };
        // En pause, la durée est celle de la dernière entrée : elle ne bouge plus.
        assert_eq!(
            menu_title(&paused, T9 + 7200, true).as_deref(),
            Some("Pause · 0:42:13")
        );
        assert_eq!(menu_title(&Status::Idle, T9, true), None);
    }

    #[test]
    fn menu_title_without_seconds_shows_hours_and_minutes() {
        let at = |secs: i64| menu_title(&running(T9), T9 + secs, false);
        assert_eq!(at(0).as_deref(), Some("0:00"));
        assert_eq!(at(59).as_deref(), Some("0:00"));
        assert_eq!(at(60).as_deref(), Some("0:01"));
        assert_eq!(at(42 * 60 + 59).as_deref(), Some("0:42"));
        assert_eq!(at(3600 + 5 * 60 + 9).as_deref(), Some("1:05"));
        assert_eq!(at(12 * 3600).as_deref(), Some("12:00"));
        assert_eq!(at(-30).as_deref(), Some("0:00"));
        let paused = Status::Paused {
            lot: lot(),
            since: T9,
            duration: 42 * 60 + 13,
            launched_by: None,
            workdir: None,
        };
        assert_eq!(
            menu_title(&paused, T9 + 7200, false).as_deref(),
            Some("Pause · 0:42")
        );
        assert_eq!(menu_title(&Status::Idle, T9, false), None);
    }

    #[test]
    fn only_an_explicit_false_hides_the_seconds() {
        assert!(shows_seconds("true"));
        assert!(!shows_seconds("false"));
        // Une valeur invalide retombe sur le défaut : les secondes.
        for invalid in ["", "oui", "False", "0"] {
            assert!(shows_seconds(invalid), "{invalid:?}");
        }
    }

    #[test]
    fn toggle_pause_pauses_a_running_lot_and_resumes_a_paused_one() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        k.clock().advance(60);
        toggle_pause(&k).unwrap();
        assert!(matches!(k.status().unwrap(), Status::Paused { .. }));
        k.clock().advance(60);
        toggle_pause(&k).unwrap();
        assert!(matches!(k.status().unwrap(), Status::Running { .. }));
    }

    #[test]
    fn toggle_pause_does_nothing_when_nothing_runs() {
        let (k, _d) = kairos();
        toggle_pause(&k).unwrap();
        assert_eq!(k.status().unwrap(), Status::Idle);
    }

    #[test]
    fn relaunch_last_restarts_the_most_recent_lot_when_idle() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        k.clock().advance(600);
        stop(&k).unwrap();
        k.clock().advance(60);
        assert!(relaunch_last(&k).unwrap());
        match k.status().unwrap() {
            Status::Running { lot, .. } => {
                assert_eq!(lot.parent_monday_id.as_deref(), Some("12406850311"));
                assert_eq!(lot.subtask.as_deref(), Some("Front"));
            }
            other => panic!("attendu : un lot en cours, obtenu {other:?}"),
        }
    }

    #[test]
    fn relaunch_last_does_nothing_without_a_recent_lot_or_when_a_lot_runs() {
        let (k, _d) = kairos();
        assert!(!relaunch_last(&k).unwrap());
        assert_eq!(k.status().unwrap(), Status::Idle);
        start(&k, request("12406850311")).unwrap();
        let before = k.status().unwrap();
        assert!(!relaunch_last(&k).unwrap());
        assert_eq!(k.status().unwrap(), before);
    }

    #[test]
    fn only_a_running_lot_asks_before_quitting() {
        assert!(lot_is_running(&running(T9)));
        assert!(!lot_is_running(&Status::Idle));
        let paused = Status::Paused {
            lot: lot(),
            since: T9,
            duration: 60,
            launched_by: None,
            workdir: None,
        };
        assert!(!lot_is_running(&paused));
    }

    #[test]
    fn quit_with_stop_lot_stops_the_running_lot() {
        let (k, _d) = kairos();
        start(&k, request("12406850311")).unwrap();
        k.clock().advance(900);
        stop_if_running(&k).unwrap();
        assert_eq!(k.status().unwrap(), Status::Idle);
        assert_eq!(k.today(None).unwrap().total, 900);
        // Rien ne tourne : quitter en arrêtant le lot n'est pas une erreur.
        stop_if_running(&k).unwrap();
    }
}
