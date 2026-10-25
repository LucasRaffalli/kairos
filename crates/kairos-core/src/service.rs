//! Le service : toutes les opérations de Kairos, chacune dans une transaction
//! `BEGIN IMMEDIATE`. Rien d'autre que ce module ne touche aux tables.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::clock::Clock;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::model::{
    AddEntry, Day, DayStatus, DayView, EditEntry, Entry, EntryView, KnownId, LotView, Overview,
    RecentLot, Source, StartArgs, StartedLot, Status, StoppedEntry, SubtaskChoices, SwitchResult,
    Totals,
};
use crate::report::{self, Report, ReportEntry};
use crate::timeline::{self, Interval};

/// Un lancement ne peut pas remonter à plus de 2 jours : au-delà, une faute de frappe créerait
/// une entrée par jour (la découpe à minuit), et `kairos add` fait ce travail proprement.
const MAX_BACKDATE_SECS: i64 = 2 * 86_400;

/// Longueur maximale d'un dossier de travail (au-delà, il est tronqué).
const MAX_WORKDIR_CHARS: usize = 500;

/// Longueur maximale d'un nom (tâche, sous-tâche, libellé).
const MAX_NAME_CHARS: usize = 120;

pub struct Kairos<C: Clock> {
    db: Db,
    clock: C,
}

/// Le contexte d'une opération : la transaction, l'instant « maintenant » (figé pour toute
/// l'opération) et le décalage local.
struct Ctx<'a> {
    tx: &'a Transaction<'a>,
    now: i64,
    offset: &'a dyn Fn(i64) -> i32,
}

impl<C: Clock> Kairos<C> {
    pub fn new(db: Db, clock: C) -> Self {
        Kairos { db, clock }
    }

    pub fn db(&self) -> &Db {
        &self.db
    }

    pub fn clock(&self) -> &C {
        &self.clock
    }

    /// Ouvre une transaction, rattrape les minuits franchis, exécute `f`, valide.
    /// Toute erreur annule tout : une opération réussit en entier ou pas du tout.
    fn with_tx<T>(&self, f: impl FnOnce(&Ctx) -> Result<T>) -> Result<T> {
        let tx = Transaction::new_unchecked(self.db.conn(), TransactionBehavior::Immediate)?;
        let offset = |at: i64| self.clock.utc_offset(at);
        let ctx = Ctx {
            tx: &tx,
            now: self.clock.now(),
            offset: &offset,
        };
        reconcile(&ctx)?;
        let out = f(&ctx)?;
        tx.commit()?;
        Ok(out)
    }

    pub fn start(&self, args: StartArgs) -> Result<StartedLot> {
        self.with_tx(|ctx| start_in(ctx, &args))
    }

    pub fn stop(&self, at: Option<i64>) -> Result<StoppedEntry> {
        self.with_tx(|ctx| {
            if open_entry(ctx)?.is_none() {
                // Rien ne tourne : arrêter une pause l'efface, sinon il n'y a rien à arrêter.
                return match paused_lot(ctx)? {
                    Some(lot_id) => {
                        clear_pause(ctx)?;
                        let entry = last_entry_of(ctx, lot_id)?.ok_or(Error::NothingRunning)?;
                        let duration = entry.end.unwrap_or(entry.start) - entry.start;
                        Ok(StoppedEntry {
                            lot: lot_view(ctx, lot_id)?,
                            entry,
                            duration,
                        })
                    }
                    None => Err(Error::NothingRunning),
                };
            }
            let stopped = close_open(ctx, at)?;
            clear_pause(ctx)?;
            Ok(stopped)
        })
    }

    pub fn pause(&self) -> Result<StoppedEntry> {
        self.with_tx(|ctx| {
            let stopped = close_open(ctx, None)?;
            ctx.tx.execute(
                "UPDATE state SET paused_lot_id = ?1 WHERE id = 1",
                [stopped.lot.id],
            )?;
            Ok(stopped)
        })
    }

    pub fn resume(&self) -> Result<StartedLot> {
        self.with_tx(|ctx| {
            let lot_id = paused_lot(ctx)?.ok_or(Error::NothingToResume)?;
            // Si l'horloge a reculé depuis la pause, reprendre chevaucherait l'entrée fermée.
            ensure_free_from(ctx, ctx.now)?;
            let origin = last_entry_of(ctx, lot_id)?
                .map(|e| Origin {
                    source: e.source,
                    workdir: e.workdir,
                })
                .unwrap_or_default();
            let entry_id = insert_open_entry(ctx, lot_id, ctx.now, &origin)?;
            clear_pause(ctx)?;
            touch_day(ctx, &timeline::local_date(ctx.now, ctx.offset))?;
            Ok(StartedLot {
                lot: lot_view(ctx, lot_id)?,
                entry: entry_by_id(ctx, entry_id)?,
            })
        })
    }

    /// Arrête le lot en cours et lance le suivant à la même seconde ; si rien ne tourne, équivaut à `start`.
    pub fn switch_to(&self, args: StartArgs) -> Result<SwitchResult> {
        self.with_tx(|ctx| {
            let reference = resolve(&args)?;
            let at = args.at.unwrap_or(ctx.now);
            let stopped = match open_entry(ctx)? {
                Some((_, running_lot, _)) => {
                    if ensure_lot(ctx, &reference, args.source)? == running_lot {
                        return Err(Error::SameLot(lot_view(ctx, running_lot)?.describe()));
                    }
                    Some(close_open(ctx, args.at)?)
                }
                None => None,
            };
            let started = start_in(
                ctx,
                &StartArgs {
                    at: Some(at),
                    ..args
                },
            )?;
            Ok(SwitchResult { stopped, started })
        })
    }

    pub fn status(&self) -> Result<Status> {
        self.with_tx(status_in)
    }

    /// La ventilation d'une journée : lignes, arrondi, recoupement avec les bornes, texte à saisir.
    pub fn report(&self, date: Option<&str>) -> Result<Report> {
        self.with_tx(|ctx| {
            let date = date_or_today(ctx, date)?;
            let status = stored_status(ctx, &date)?;
            let rounding: i64 = ctx
                .tx
                .query_row(
                    "SELECT value FROM settings WHERE key = 'rounding_minutes'",
                    [],
                    |r| r.get::<_, String>(0),
                )?
                .parse()
                .unwrap_or(0);
            let declared: Option<Totals> = match status {
                DayStatus::ToRedeclare => {
                    let json: Option<String> = ctx.tx.query_row(
                        "SELECT declared_totals FROM days WHERE date = ?1",
                        [&date],
                        |r| r.get(0),
                    )?;
                    json.and_then(|j| serde_json::from_str(&j).ok())
                }
                _ => None,
            };
            let entries: Vec<ReportEntry> = day_entries(ctx, &date)?
                .into_iter()
                .filter(|(e, _)| !e.removed)
                .map(|(e, lot)| ReportEntry {
                    id: e.id,
                    start: e.start,
                    end: e.end.unwrap_or(ctx.now).max(e.start),
                    parent_monday_id: lot.parent_monday_id,
                    subtask: lot.subtask,
                    name: lot.name,
                })
                .collect();
            Ok(report::build(
                &date,
                status,
                &entries,
                rounding,
                declared.as_ref(),
            ))
        })
    }

    /// Ajoute un temps oublié, après coup.
    pub fn add_entry(&self, add: AddEntry) -> Result<Entry> {
        self.with_tx(|ctx| {
            let reference = resolve(&StartArgs {
                parent_monday_id: add.parent_monday_id,
                parent_label: add.parent_label,
                name: add.name,
                subtask: add.subtask,
                ..StartArgs::default()
            })?;
            check_range(ctx, add.from, add.to, None)?;
            let lot_id = ensure_lot(ctx, &reference, crate::model::Source::Manual)?;
            ctx.tx.execute(
                "INSERT INTO entries (lot_id, start_at, end_at) VALUES (?1, ?2, ?3)",
                params![lot_id, add.from, add.to],
            )?;
            let id = ctx.tx.last_insert_rowid();
            touch_day(ctx, &timeline::local_date(add.from, ctx.offset))?;
            entry_by_id(ctx, id)
        })
    }

    /// Corrige une entrée : mêmes règles qu'à l'ajout, et la journée repasse « à resaisir » si elle était saisie.
    pub fn edit_entry(&self, id: i64, edit: EditEntry) -> Result<Entry> {
        self.with_tx(|ctx| {
            let entry = entry_by_id(ctx, id)?;
            let from = edit.from.unwrap_or(entry.start);
            let to = edit.to.or(entry.end);
            check_range(ctx, from, to.unwrap_or(ctx.now), Some(id))?;
            let mut lot_id = entry.lot_id;
            if edit.parent_monday_id.is_some() || edit.name.is_some() || edit.subtask.is_some() {
                let current = lot_view(ctx, entry.lot_id)?;
                let reference = resolve(&StartArgs {
                    parent_monday_id: edit.parent_monday_id.or(current.parent_monday_id),
                    name: edit.name.or(current.name),
                    subtask: edit.subtask.or(current.subtask),
                    ..StartArgs::default()
                })?;
                lot_id = ensure_lot(ctx, &reference, crate::model::Source::Manual)?;
            }
            ctx.tx.execute(
                "UPDATE entries SET lot_id = ?1, start_at = ?2, end_at = ?3 WHERE id = ?4",
                params![lot_id, from, to, id],
            )?;
            touch_day(ctx, &timeline::local_date(entry.start, ctx.offset))?;
            touch_day(ctx, &timeline::local_date(from, ctx.offset))?;
            entry_by_id(ctx, id)
        })
    }

    pub fn delete_entry(&self, id: i64) -> Result<()> {
        self.with_tx(|ctx| {
            let entry = entry_by_id(ctx, id)?;
            ctx.tx.execute("DELETE FROM entries WHERE id = ?1", [id])?;
            touch_day(ctx, &timeline::local_date(entry.start, ctx.offset))
        })
    }

    /// Les entrées d'une journée (aujourd'hui par défaut), leurs bornes et leur total.
    pub fn today(&self, date: Option<&str>) -> Result<DayView> {
        self.with_tx(|ctx| {
            let date = date_or_today(ctx, date)?;
            day_view(ctx, &date)
        })
    }

    /// Clôture une journée : fige ses totaux. Refusée si un lot y tourne encore, sauf `stop_running`.
    pub fn close_day(&self, date: Option<&str>, stop_running: bool) -> Result<Day> {
        self.with_tx(|ctx| {
            let date = date_or_today(ctx, date)?;
            let bounds = timeline::day_bounds(&date, ctx.offset)?;
            if stored_status(ctx, &date)? != DayStatus::Open {
                return Err(Error::DayNotOpen(date));
            }
            let entries: i64 = ctx.tx.query_row("SELECT COUNT(*) FROM entries WHERE removed = 0 AND start_at >= ?1 AND start_at < ?2", [bounds.start, bounds.end], |r| r.get(0))?;
            if entries == 0 {
                return Err(Error::DayEmpty(date));
            }
            if let Some((_, lot_id, start)) = open_entry(ctx)? {
                if start >= bounds.start && start < bounds.end {
                    if !stop_running {
                        return Err(Error::RunningLotOnClose(lot_view(ctx, lot_id)?.describe()));
                    }
                    close_open(ctx, None)?;
                }
            }
            let totals = totals(ctx, &date)?;
            ctx.tx.execute(
                "INSERT INTO days (date, status, closed_at, closed_totals) VALUES (?1, 'closed', ?2, ?3) ON CONFLICT(date) DO UPDATE SET status = 'closed', closed_at = ?2, closed_totals = ?3",
                params![date, ctx.now, totals_json(&totals)],
            )?;
            day_row(ctx, &date)
        })
    }

    /// Note que l'utilisateur a saisi la journée dans Monday.
    pub fn mark_declared(&self, date: Option<&str>) -> Result<Day> {
        self.with_tx(|ctx| {
            let date = date_or_today(ctx, date)?;
            match stored_status(ctx, &date)? {
                DayStatus::Open => return Err(Error::DayNotClosed(date)),
                DayStatus::Declared => return day_row(ctx, &date),
                DayStatus::Closed | DayStatus::ToRedeclare => {}
            }
            let totals = totals(ctx, &date)?;
            ctx.tx.execute("UPDATE days SET status = 'declared', declared_at = ?1, declared_totals = ?2 WHERE date = ?3", params![ctx.now, totals_json(&totals), date])?;
            day_row(ctx, &date)
        })
    }

    /// Mémorise l'ID Monday d'une sous-tâche d'un parent, collé à la main.
    pub fn set_subtask_id(
        &self,
        parent_monday_id: &str,
        subtask: &str,
        monday_item_id: &str,
    ) -> Result<()> {
        self.with_tx(|ctx| {
            let parent = clean_monday_id(&Some(parent_monday_id.to_string()))?
                .ok_or_else(|| Error::InvalidMondayId(parent_monday_id.to_string()))?;
            let item = clean_monday_id(&Some(monday_item_id.to_string()))?
                .ok_or_else(|| Error::InvalidMondayId(monday_item_id.to_string()))?;
            let name = clean_text(&Some(subtask.to_string()), "La sous-tâche")?
                .ok_or_else(|| Error::InvalidName("la sous-tâche est vide".into()))?;
            let project = default_project(ctx)?;
            let parent_row = ensure_parent(ctx, project, &parent, None)?;
            let row = own_subtask(ctx, project, parent_row, &name)?;
            ctx.tx.execute(
                "UPDATE subtasks SET monday_item_id = ?1 WHERE id = ?2",
                params![item, row],
            )?;
            Ok(())
        })
    }

    /// Ce que Kairos sait d'un ID Monday : parent connu, sous-tâche mémorisée, ou rien. Lecture seule.
    pub fn resolve_monday_id(&self, id: &str) -> Result<KnownId> {
        self.with_tx(|ctx| {
            let id = clean_monday_id(&Some(id.to_string()))?
                .ok_or_else(|| Error::InvalidMondayId(id.to_string()))?;
            let parent: Option<Option<String>> = ctx
                .tx
                .query_row(
                    "SELECT label FROM parents WHERE monday_id = ?1",
                    [&id],
                    |r| r.get(0),
                )
                .optional()?;
            if let Some(label) = parent {
                return Ok(KnownId::Parent { label });
            }
            let subtask = ctx
                .tx
                .query_row(
                    "SELECT p.monday_id, s.name, p.label FROM subtasks s JOIN parents p ON p.id = s.parent_id WHERE s.monday_item_id = ?1 AND p.monday_id IS NOT NULL",
                    [&id],
                    |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
                )
                .optional()?;
            Ok(match subtask {
                Some((parent_monday_id, subtask, label)) => KnownId::Subtask {
                    parent_monday_id,
                    subtask,
                    label,
                },
                None => KnownId::Unknown,
            })
        })
    }

    /// Les sous-tâches à proposer pour un parent : le poste principal, les sous-tâches de base et
    /// celles propres à ce parent (aucune si on ne le connaît pas). Lecture seule.
    pub fn subtask_choices(&self, parent_monday_id: Option<&str>) -> Result<SubtaskChoices> {
        self.with_tx(|ctx| subtask_choices_in(ctx, parent_monday_id))
    }

    /// Les lots des 7 derniers jours, un par lot, du plus récent au plus ancien.
    pub fn recent_lots(&self, limit: usize) -> Result<Vec<RecentLot>> {
        self.with_tx(|ctx| recent_lots_in(ctx, limit))
    }

    /// Tout ce qu'il faut pour dessiner l'app, lu dans **une** transaction (jamais un état
    /// mélangé) : le statut, la journée, jusqu'à `recent_limit` lots récents et les sous-tâches
    /// de `parent_monday_id`, à défaut du lot en cours ou en pause, à défaut du poste principal.
    pub fn overview(
        &self,
        recent_limit: usize,
        parent_monday_id: Option<&str>,
    ) -> Result<Overview> {
        self.with_tx(|ctx| {
            let status = status_in(ctx)?;
            let current_parent = match &status {
                Status::Running { lot, .. } | Status::Paused { lot, .. } => {
                    lot.parent_monday_id.clone()
                }
                Status::Idle => None,
            };
            let parent = parent_monday_id.or(current_parent.as_deref());
            Ok(Overview {
                today: day_view(ctx, &timeline::local_date(ctx.now, ctx.offset))?,
                recent: recent_lots_in(ctx, recent_limit)?,
                choices: subtask_choices_in(ctx, parent)?,
                status,
            })
        })
    }

    /// Revient au lot d'avant : celui de la dernière entrée fermée dont le lot diffère de celui en
    /// cours ; si rien ne tourne, relance le lot de la dernière entrée fermée.
    pub fn back(&self, source: Source, workdir: Option<String>) -> Result<SwitchResult> {
        let target = self.with_tx(|ctx| {
            // Le lot en pause compte comme le lot courant, comme dans `recent_lots`.
            let running = match open_entry(ctx)? {
                Some((_, lot_id, _)) => Some(lot_id),
                None => paused_lot(ctx)?,
            };
            let lot_id: Option<i64> = ctx
                .tx
                .query_row(
                    "SELECT lot_id FROM entries WHERE removed = 0 AND end_at IS NOT NULL AND (?1 IS NULL OR lot_id <> ?1) ORDER BY end_at DESC, id DESC LIMIT 1",
                    [running],
                    |r| r.get(0),
                )
                .optional()?;
            let lot = lot_view(ctx, lot_id.ok_or(Error::NoPreviousLot)?)?;
            Ok(StartArgs {
                parent_monday_id: lot.parent_monday_id,
                name: lot.name,
                subtask: lot.subtask,
                source,
                workdir,
                ..StartArgs::default()
            })
        })?;
        self.switch_to(target)
    }

    /// Pose ou remplace le libellé d'un parent connu ; rend le libellé tel qu'il est enregistré
    /// (espaces retirés).
    pub fn set_label(&self, parent_monday_id: &str, label: &str) -> Result<String> {
        self.with_tx(|ctx| {
            let id = clean_monday_id(&Some(parent_monday_id.to_string()))?
                .ok_or_else(|| Error::InvalidMondayId(parent_monday_id.to_string()))?;
            let label = clean_text(&Some(label.to_string()), "Le libellé")?
                .ok_or_else(|| Error::InvalidName("le libellé est vide".into()))?;
            let changed = ctx.tx.execute(
                "UPDATE parents SET label = ?1 WHERE monday_id = ?2",
                params![label, id],
            )?;
            if changed == 0 {
                return Err(Error::ParentNotFound(id));
            }
            Ok(label)
        })
    }

    /// Une entrée avec son lot, pour décrire ce qu'une suppression retirerait. Lecture seule.
    pub fn entry_view(&self, id: i64) -> Result<EntryView> {
        self.with_tx(|ctx| {
            let entry = entry_by_id(ctx, id)?;
            Ok(EntryView {
                id: entry.id,
                lot: lot_view(ctx, entry.lot_id)?,
                start: entry.start,
                end: entry.end,
                removed: entry.removed,
            })
        })
    }

    pub fn config_get(&self, key: &str) -> Result<String> {
        self.with_tx(|ctx| {
            check_setting_key(key)?;
            let stored: Option<String> = ctx
                .tx
                .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                    r.get(0)
                })
                .optional()?;
            // Les réglages ajoutés après la première version n'ont pas de ligne tant qu'on ne les
            // a pas écrits (pas de migration, donc les anciennes versions restent compatibles).
            Ok(stored.unwrap_or_else(|| default_setting(key).to_string()))
        })
    }

    pub fn config_set(&self, key: &str, value: &str) -> Result<()> {
        self.with_tx(|ctx| {
            check_setting_key(key)?;
            let value = match key {
                "rounding_minutes" if ["0", "5", "10", "15"].contains(&value) => value.to_string(),
                "rounding_minutes" => {
                    return Err(Error::InvalidSetting(format!(
                        "rounding_minutes vaut 0, 5, 10 ou 15 (reçu « {value} »)"
                    )))
                }
                "claude_autonomy" if ["ask", "auto"].contains(&value) => value.to_string(),
                "claude_autonomy" => {
                    return Err(Error::InvalidSetting(format!(
                        "claude_autonomy vaut ask (Claude demande avant les actions difficiles à défaire) ou auto (il agit seul) ; reçu « {value} »"
                    )))
                }
                "menu_title_seconds" if ["true", "false"].contains(&value) => value.to_string(),
                "menu_title_seconds" => {
                    return Err(Error::InvalidSetting(format!(
                        "menu_title_seconds vaut true (H:MM:SS dans la barre de menus) ou false (H:MM) ; reçu « {value} »"
                    )))
                }
                _ => clean_text(&Some(value.to_string()), "Le poste principal")?.ok_or_else(
                    || Error::InvalidSetting("main_subtask ne peut pas être vide".into()),
                )?,
            };
            ctx.tx.execute(
                "INSERT INTO settings (key, value) VALUES (?2, ?1)
                 ON CONFLICT(key) DO UPDATE SET value = excluded.value",
                params![value, key],
            )?;
            Ok(())
        })
    }
}

// --- Lots -------------------------------------------------------------------

struct Reference {
    parent_monday_id: Option<String>,
    parent_label: Option<String>,
    name: Option<String>,
    subtask: Option<String>,
    subtask_monday_id: Option<String>,
}

/// Un texte libre : rogné, sur une seule ligne, 120 caractères au plus ; vide = absent.
fn clean_text(raw: &Option<String>, what: &str) -> Result<Option<String>> {
    let Some(raw) = raw else { return Ok(None) };
    let text = raw.trim();
    if text.is_empty() {
        return Ok(None);
    }
    if text.chars().any(char::is_control) {
        return Err(Error::InvalidName(format!(
            "{what} doit tenir sur une seule ligne"
        )));
    }
    if text.chars().count() > MAX_NAME_CHARS {
        return Err(Error::InvalidName(format!(
            "{what} : {MAX_NAME_CHARS} caractères au plus"
        )));
    }
    Ok(Some(text.to_string()))
}

/// Un ID Monday : chiffres seulement, 1 à 20 ; vide = absent.
pub(crate) fn clean_monday_id(raw: &Option<String>) -> Result<Option<String>> {
    let Some(raw) = raw else { return Ok(None) };
    let id = raw.trim();
    if id.is_empty() {
        return Ok(None);
    }
    if id.len() > 20 || !id.bytes().all(|b| b.is_ascii_digit()) {
        return Err(Error::InvalidMondayId(id.to_string()));
    }
    Ok(Some(id.to_string()))
}

fn resolve(args: &StartArgs) -> Result<Reference> {
    let reference = Reference {
        parent_monday_id: clean_monday_id(&args.parent_monday_id)?,
        parent_label: clean_text(&args.parent_label, "Le libellé")?,
        name: clean_text(&args.name, "Le nom de la tâche")?,
        subtask: clean_text(&args.subtask, "La sous-tâche")?,
        subtask_monday_id: clean_monday_id(&args.subtask_monday_id)?,
    };
    if reference.parent_monday_id.is_none() && reference.name.is_none() {
        return Err(Error::ReferenceMissing);
    }
    Ok(reference)
}

fn default_project(ctx: &Ctx) -> Result<i64> {
    Ok(ctx
        .tx
        .query_row("SELECT id FROM projects WHERE is_default = 1", [], |r| {
            r.get(0)
        })?)
}

/// Trouve ou crée le parent, la sous-tâche et le lot ; renvoie l'id du lot.
fn ensure_lot(ctx: &Ctx, r: &Reference, source: crate::model::Source) -> Result<i64> {
    let project = default_project(ctx)?;

    let parent_row: Option<i64> = match &r.parent_monday_id {
        Some(monday_id) => Some(ensure_parent(
            ctx,
            project,
            monday_id,
            r.parent_label.as_deref(),
        )?),
        None => None,
    };

    // Avec un ID Monday, la sous-tâche est obligatoire : à défaut, le poste principal.
    let subtask_name = match (&r.subtask, parent_row) {
        (Some(name), _) => Some(name.clone()),
        (None, Some(_)) => Some(ctx.tx.query_row(
            "SELECT value FROM settings WHERE key = 'main_subtask'",
            [],
            |row| row.get::<_, String>(0),
        )?),
        (None, None) => None,
    };
    let subtask_row: Option<i64> = match subtask_name {
        Some(name) => Some(ensure_subtask(ctx, project, parent_row, &name)?),
        None => None,
    };
    // L'ID Monday d'une sous-tâche est propre à son parent, même pour une sous-tâche de base.
    let subtask_row = match (&r.subtask_monday_id, parent_row, subtask_row) {
        (Some(item), Some(parent), Some(shared)) => {
            let own = own_subtask(ctx, project, parent, &subtask_name_of(ctx, shared)?)?;
            ctx.tx.execute(
                "UPDATE subtasks SET monday_item_id = ?1 WHERE id = ?2",
                params![item, own],
            )?;
            Some(own)
        }
        (Some(item), None, Some(id)) => {
            ctx.tx.execute(
                "UPDATE subtasks SET monday_item_id = ?1 WHERE id = ?2",
                params![item, id],
            )?;
            Some(id)
        }
        _ => subtask_row,
    };

    let existing: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT id FROM lots WHERE project_id = ?1 AND COALESCE(parent_id, 0) = COALESCE(?2, 0) AND COALESCE(subtask_id, 0) = COALESCE(?3, 0) AND COALESCE(name, '') = COALESCE(?4, '')",
            params![project, parent_row, subtask_row, r.name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = existing {
        return Ok(id);
    }
    ctx.tx.execute("INSERT INTO lots (project_id, parent_id, subtask_id, name, source) VALUES (?1, ?2, ?3, ?4, ?5)", params![project, parent_row, subtask_row, r.name, source.as_str()])?;
    Ok(ctx.tx.last_insert_rowid())
}

/// Trouve ou crée un parent par son ID Monday ; mémorise son libellé et la dernière fois qu'on l'a vu.
fn ensure_parent(ctx: &Ctx, project: i64, monday_id: &str, label: Option<&str>) -> Result<i64> {
    let existing: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT id FROM parents WHERE project_id = ?1 AND monday_id = ?2",
            params![project, monday_id],
            |row| row.get(0),
        )
        .optional()?;
    let id = match existing {
        Some(id) => id,
        None => {
            ctx.tx.execute(
                "INSERT INTO parents (project_id, monday_id) VALUES (?1, ?2)",
                params![project, monday_id],
            )?;
            ctx.tx.last_insert_rowid()
        }
    };
    ctx.tx.execute(
        "UPDATE parents SET last_seen = ?1, label = COALESCE(?2, label) WHERE id = ?3",
        params![ctx.now, label, id],
    )?;
    Ok(id)
}

/// La sous-tâche d'un lot : celle du parent si elle existe, sinon la sous-tâche de base du même
/// nom (partagée), sinon une nouvelle, propre à ce parent.
fn ensure_subtask(ctx: &Ctx, project: i64, parent: Option<i64>, name: &str) -> Result<i64> {
    if let Some(parent) = parent {
        let own: Option<i64> = ctx
            .tx
            .query_row(
                "SELECT id FROM subtasks WHERE project_id = ?1 AND parent_id = ?2 AND name = ?3",
                params![project, parent, name],
                |row| row.get(0),
            )
            .optional()?;
        if let Some(id) = own {
            return Ok(id);
        }
    }
    let shared: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT id FROM subtasks WHERE project_id = ?1 AND parent_id IS NULL AND name = ?2",
            params![project, name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = shared {
        return Ok(id);
    }
    ctx.tx.execute(
        "INSERT INTO subtasks (project_id, parent_id, name, is_base) VALUES (?1, ?2, ?3, 0)",
        params![project, parent, name],
    )?;
    Ok(ctx.tx.last_insert_rowid())
}

/// La sous-tâche **propre** à un parent (créée au besoin, même si une sous-tâche de base porte ce nom) :
/// c'est elle qui porte l'ID Monday, différent d'un parent à l'autre.
fn own_subtask(ctx: &Ctx, project: i64, parent: i64, name: &str) -> Result<i64> {
    let own: Option<i64> = ctx
        .tx
        .query_row(
            "SELECT id FROM subtasks WHERE project_id = ?1 AND parent_id = ?2 AND name = ?3",
            params![project, parent, name],
            |row| row.get(0),
        )
        .optional()?;
    if let Some(id) = own {
        return Ok(id);
    }
    ctx.tx.execute(
        "INSERT INTO subtasks (project_id, parent_id, name, is_base) VALUES (?1, ?2, ?3, 0)",
        params![project, parent, name],
    )?;
    Ok(ctx.tx.last_insert_rowid())
}

fn subtask_name_of(ctx: &Ctx, id: i64) -> Result<String> {
    Ok(ctx
        .tx
        .query_row("SELECT name FROM subtasks WHERE id = ?1", [id], |r| {
            r.get(0)
        })?)
}

fn lot_view(ctx: &Ctx, lot_id: i64) -> Result<LotView> {
    Ok(ctx.tx.query_row(
        "SELECT l.id, p.monday_id, p.label, s.name, s.monday_item_id, l.name FROM lots l LEFT JOIN parents p ON p.id = l.parent_id LEFT JOIN subtasks s ON s.id = l.subtask_id WHERE l.id = ?1",
        [lot_id],
        |row| Ok(LotView { id: row.get(0)?, parent_monday_id: row.get(1)?, parent_label: row.get(2)?, subtask: row.get(3)?, subtask_monday_id: row.get(4)?, name: row.get(5)? }),
    )?)
}

// --- Entrées ----------------------------------------------------------------

/// « 13 h 34 » : l'heure locale d'un instant, pour les messages.
fn fmt_hm(ctx: &Ctx, at: i64) -> String {
    let secs = (at + i64::from((ctx.offset)(at))).rem_euclid(86_400);
    format!("{} h {:02}", secs / 3600, (secs % 3600) / 60)
}

fn open_entry(ctx: &Ctx) -> Result<Option<(i64, i64, i64)>> {
    Ok(ctx
        .tx
        .query_row(
            "SELECT id, lot_id, start_at FROM entries WHERE end_at IS NULL",
            [],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?)
}

fn entry_by_id(ctx: &Ctx, id: i64) -> Result<Entry> {
    ctx.tx
        .query_row(
            "SELECT id, lot_id, start_at, end_at, removed, note, source, workdir FROM entries WHERE id = ?1",
            [id],
            row_to_entry,
        )
        .optional()?
        .ok_or(Error::EntryNotFound(id))
}

fn row_to_entry(r: &rusqlite::Row) -> rusqlite::Result<Entry> {
    Ok(Entry {
        id: r.get(0)?,
        lot_id: r.get(1)?,
        start: r.get(2)?,
        end: r.get(3)?,
        removed: r.get::<_, i64>(4)? != 0,
        note: r.get(5)?,
        source: r
            .get::<_, Option<String>>(6)?
            .as_deref()
            .and_then(Source::parse),
        workdir: r.get(7)?,
    })
}

fn last_entry_of(ctx: &Ctx, lot_id: i64) -> Result<Option<Entry>> {
    Ok(ctx.tx.query_row("SELECT id, lot_id, start_at, end_at, removed, note, source, workdir FROM entries WHERE lot_id = ?1 ORDER BY start_at DESC LIMIT 1", [lot_id], row_to_entry).optional()?)
}

/// Qui a lancé une entrée, et d'où.
#[derive(Default)]
struct Origin {
    source: Option<Source>,
    workdir: Option<String>,
}

fn entry_origin(ctx: &Ctx, id: i64) -> Result<Origin> {
    let entry = entry_by_id(ctx, id)?;
    Ok(Origin {
        source: entry.source,
        workdir: entry.workdir,
    })
}

/// Un dossier de travail propre : rogné, sans caractère de contrôle (sinon ignoré), 500
/// caractères au plus. Ce n'est qu'une information d'affichage : jamais une erreur.
fn clean_workdir(raw: &Option<String>) -> Option<String> {
    let text = raw.as_deref()?.trim();
    if text.is_empty() || text.chars().any(char::is_control) {
        return None;
    }
    Some(text.chars().take(MAX_WORKDIR_CHARS).collect())
}

fn insert_open_entry(ctx: &Ctx, lot_id: i64, at: i64, origin: &Origin) -> Result<i64> {
    ctx.tx.execute(
        "INSERT INTO entries (lot_id, start_at, source, workdir) VALUES (?1, ?2, ?3, ?4)",
        params![
            lot_id,
            at,
            origin.source.map(Source::as_str),
            origin.workdir
        ],
    )?;
    Ok(ctx.tx.last_insert_rowid())
}

fn paused_lot(ctx: &Ctx) -> Result<Option<i64>> {
    Ok(ctx
        .tx
        .query_row("SELECT paused_lot_id FROM state WHERE id = 1", [], |r| {
            r.get(0)
        })?)
}

fn clear_pause(ctx: &Ctx) -> Result<()> {
    ctx.tx
        .execute("UPDATE state SET paused_lot_id = NULL WHERE id = 1", [])?;
    Ok(())
}

/// Ce que fait Kairos à cet instant, dans la transaction en cours.
fn status_in(ctx: &Ctx) -> Result<Status> {
    if let Some((entry_id, lot_id, since)) = open_entry(ctx)? {
        let today = timeline::local_date(ctx.now, ctx.offset);
        let origin = entry_origin(ctx, entry_id)?;
        return Ok(Status::Running {
            lot: lot_view(ctx, lot_id)?,
            since,
            duration: (ctx.now - since).max(0),
            today_total: day_total(ctx, &today)?,
            launched_by: origin.source,
            workdir: origin.workdir,
        });
    }
    if let Some(lot_id) = paused_lot(ctx)? {
        let last = last_entry_of(ctx, lot_id)?;
        let since = last.as_ref().and_then(|e| e.end).unwrap_or(ctx.now);
        let duration = last.as_ref().map_or(0, |e| (since - e.start).max(0));
        return Ok(Status::Paused {
            lot: lot_view(ctx, lot_id)?,
            since,
            duration,
            launched_by: last.as_ref().and_then(|e| e.source),
            workdir: last.and_then(|e| e.workdir),
        });
    }
    Ok(Status::Idle)
}

/// Les sous-tâches à proposer pour un parent, dans la transaction en cours.
fn subtask_choices_in(ctx: &Ctx, parent_monday_id: Option<&str>) -> Result<SubtaskChoices> {
    let parent = clean_monday_id(&parent_monday_id.map(str::to_string))?;
    let main = ctx.tx.query_row(
        "SELECT value FROM settings WHERE key = 'main_subtask'",
        [],
        |r| r.get(0),
    )?;
    let names = |sql: &str, params: &[&dyn rusqlite::ToSql]| -> Result<Vec<String>> {
        let mut stmt = ctx.tx.prepare(sql)?;
        let rows = stmt.query_map(params, |r| r.get(0))?;
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    };
    Ok(SubtaskChoices {
        main,
        base: names(
            "SELECT name FROM subtasks WHERE parent_id IS NULL AND is_base = 1 ORDER BY id",
            &[],
        )?,
        own: names(
            "SELECT s.name FROM subtasks s JOIN parents p ON p.id = s.parent_id WHERE p.monday_id = ?1 ORDER BY s.id",
            &[&parent],
        )?,
    })
}

/// Les lots récents, dans la transaction en cours.
fn recent_lots_in(ctx: &Ctx, limit: usize) -> Result<Vec<RecentLot>> {
    let since = ctx.now - 7 * 86_400;
    let mut stmt = ctx.tx.prepare(
        "SELECT lot_id, MAX(COALESCE(end_at, ?2)) AS last_end FROM entries WHERE removed = 0 AND COALESCE(end_at, ?2) >= ?1 GROUP BY lot_id ORDER BY last_end DESC, MAX(id) DESC LIMIT ?3",
    )?;
    let rows: Vec<(i64, i64)> = stmt
        .query_map(
            params![since, ctx.now, i64::try_from(limit).unwrap_or(i64::MAX)],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )?
        .collect::<rusqlite::Result<_>>()?;
    let current = match open_entry(ctx)? {
        Some((_, lot_id, _)) => Some(lot_id),
        None => paused_lot(ctx)?,
    };
    let today = timeline::local_date(ctx.now, ctx.offset);
    let mut recent = Vec::new();
    for (lot_id, last_end) in rows {
        recent.push(RecentLot {
            lot: lot_view(ctx, lot_id)?,
            last_end,
            today_total: lot_day_total(ctx, lot_id, &today)?,
            current: current == Some(lot_id),
        });
    }
    Ok(recent)
}

/// Coupe l'entrée ouverte à chaque minuit local franchi : une entrée par jour, sur le même lot.
fn reconcile(ctx: &Ctx) -> Result<()> {
    let Some((id, lot_id, start)) = open_entry(ctx)? else {
        return Ok(());
    };
    let midnights = timeline::midnights_between(start, ctx.now, ctx.offset);
    let Some(&first) = midnights.first() else {
        return Ok(());
    };
    // Chaque morceau garde l'origine de l'entrée qu'on coupe.
    let origin = entry_origin(ctx, id)?;
    ctx.tx.execute(
        "UPDATE entries SET end_at = ?1 WHERE id = ?2",
        params![first, id],
    )?;
    for pair in midnights.windows(2) {
        ctx.tx.execute(
            "INSERT INTO entries (lot_id, start_at, end_at, source, workdir) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![
                lot_id,
                pair[0],
                pair[1],
                origin.source.map(Source::as_str),
                origin.workdir
            ],
        )?;
    }
    insert_open_entry(
        ctx,
        lot_id,
        *midnights.last().expect("liste non vide"),
        &origin,
    )?;
    Ok(())
}

/// Refuse un début qui chevaucherait une entrée déjà fermée. Des bornes qui se touchent passent.
fn ensure_free_from(ctx: &Ctx, at: i64) -> Result<()> {
    let clash: Option<(i64, i64, i64)> = ctx
        .tx
        .query_row("SELECT start_at, end_at, lot_id FROM entries WHERE removed = 0 AND end_at IS NOT NULL AND end_at > ?1 ORDER BY start_at LIMIT 1", [at], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    if let Some((start, end, lot_id)) = clash {
        return Err(Error::Overlap {
            with: format!(
                "{} – {} sur {}",
                fmt_hm(ctx, start),
                fmt_hm(ctx, end),
                lot_view(ctx, lot_id)?.target()
            ),
        });
    }
    Ok(())
}

fn start_in(ctx: &Ctx, args: &StartArgs) -> Result<StartedLot> {
    let reference = resolve(args)?;
    if let Some((_, lot_id, since)) = open_entry(ctx)? {
        return Err(Error::AlreadyRunning {
            lot: lot_view(ctx, lot_id)?.describe(),
            since: fmt_hm(ctx, since),
        });
    }
    let at = args.at.unwrap_or(ctx.now);
    if at > ctx.now {
        return Err(Error::InFuture {
            at: fmt_hm(ctx, at),
            now: fmt_hm(ctx, ctx.now),
        });
    }
    if at < ctx.now - MAX_BACKDATE_SECS {
        return Err(Error::TooOld {
            at: format!(
                "{} {}",
                timeline::local_date(at, ctx.offset),
                fmt_hm(ctx, at)
            ),
        });
    }
    ensure_free_from(ctx, at)?;
    let lot_id = ensure_lot(ctx, &reference, args.source)?;
    let origin = Origin {
        source: Some(args.source),
        workdir: clean_workdir(&args.workdir),
    };
    let entry_id = insert_open_entry(ctx, lot_id, at, &origin)?;
    clear_pause(ctx)?;
    touch_day(ctx, &timeline::local_date(at, ctx.offset))?;
    // Un lancement en retard peut traverser un minuit : on coupe tout de suite.
    reconcile(ctx)?;
    let entry = match open_entry(ctx)? {
        Some((id, _, _)) if id != entry_id => entry_by_id(ctx, id)?,
        _ => entry_by_id(ctx, entry_id)?,
    };
    Ok(StartedLot {
        lot: lot_view(ctx, lot_id)?,
        entry,
    })
}

/// Ferme l'entrée ouverte à `at` (ou maintenant). Une entrée de durée nulle est supprimée :
/// rien n'a été chronométré. Sans lot en cours : `NothingRunning`.
fn close_open(ctx: &Ctx, at: Option<i64>) -> Result<StoppedEntry> {
    let (id, lot_id, start) = open_entry(ctx)?.ok_or(Error::NothingRunning)?;
    let explicit = at.is_some();
    let end = at.unwrap_or(ctx.now);
    if end > ctx.now {
        return Err(Error::InFuture {
            at: fmt_hm(ctx, end),
            now: fmt_hm(ctx, ctx.now),
        });
    }
    if end < start || (explicit && end == start) {
        // Une heure d'arrêt avant le début de l'entrée ouverte : si elle vient d'une coupure de
        // minuit, c'est « j'ai oublié d'arrêter hier soir » et l'heure tombe dans une entrée précédente.
        return if explicit {
            close_into_earlier_piece(ctx, id, lot_id, start, end)
        } else {
            Err(Error::ClockBehind {
                since: fmt_hm(ctx, start),
            })
        };
    }
    let lot = lot_view(ctx, lot_id)?;
    touch_day(ctx, &timeline::local_date(start, ctx.offset))?;
    if end == start {
        ctx.tx.execute("DELETE FROM entries WHERE id = ?1", [id])?;
        return Ok(StoppedEntry {
            lot,
            entry: Entry {
                id,
                lot_id,
                start,
                end: Some(start),
                removed: false,
                note: None,
                source: None,
                workdir: None,
            },
            duration: 0,
        });
    }
    ctx.tx.execute(
        "UPDATE entries SET end_at = ?1 WHERE id = ?2",
        params![end, id],
    )?;
    Ok(StoppedEntry {
        lot,
        entry: entry_by_id(ctx, id)?,
        duration: end - start,
    })
}

/// Arrête un lot à une heure qui tombe avant le début de l'entrée ouverte, dans une entrée
/// précédente du même lot (les morceaux d'une coupure de minuit se suivent bout à bout) :
/// cette entrée est coupée à `at`, les morceaux suivants et l'entrée ouverte sont supprimés.
fn close_into_earlier_piece(
    ctx: &Ctx,
    open_id: i64,
    lot_id: i64,
    open_start: i64,
    at: i64,
) -> Result<StoppedEntry> {
    let mut chain: Vec<(i64, i64, i64)> = Vec::new(); // (id, début, fin), du plus récent au plus ancien
    let mut cursor = open_start;
    while let Some((id, start)) = ctx
        .tx
        .query_row(
            "SELECT id, start_at FROM entries WHERE lot_id = ?1 AND removed = 0 AND end_at = ?2",
            params![lot_id, cursor],
            |r| Ok((r.get::<_, i64>(0)?, r.get::<_, i64>(1)?)),
        )
        .optional()?
    {
        chain.push((id, start, cursor));
        cursor = start;
    }
    let hit = chain
        .iter()
        .position(|&(_, start, end)| start < at && at <= end)
        .ok_or(Error::EmptyEntry)?;
    let (piece_id, piece_start, piece_end) = chain[hit];
    for &(id, start, _) in &chain[..hit] {
        ctx.tx.execute("DELETE FROM entries WHERE id = ?1", [id])?;
        touch_day(ctx, &timeline::local_date(start, ctx.offset))?;
    }
    ctx.tx
        .execute("DELETE FROM entries WHERE id = ?1", [open_id])?;
    touch_day(ctx, &timeline::local_date(open_start, ctx.offset))?;
    if at < piece_end {
        ctx.tx.execute(
            "UPDATE entries SET end_at = ?1 WHERE id = ?2",
            params![at, piece_id],
        )?;
    }
    touch_day(ctx, &timeline::local_date(piece_start, ctx.offset))?;
    Ok(StoppedEntry {
        lot: lot_view(ctx, lot_id)?,
        entry: entry_by_id(ctx, piece_id)?,
        duration: at - piece_start,
    })
}

/// Le temps chronométré un jour local, entrées ouvertes comprises (comptées jusqu'à maintenant).
fn day_total(ctx: &Ctx, date: &str) -> Result<i64> {
    let day: Interval = timeline::day_bounds(date, ctx.offset)?;
    let mut stmt = ctx.tx.prepare("SELECT start_at, end_at FROM entries WHERE removed = 0 AND start_at < ?1 AND COALESCE(end_at, ?3) > ?2")?;
    let rows = stmt.query_map(params![day.end, day.start, ctx.now], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
    })?;
    let mut total = 0;
    for row in rows {
        let (start, end) = row?;
        let end = end.unwrap_or(ctx.now).min(day.end);
        total += (end - start.max(day.start)).max(0);
    }
    Ok(total)
}

/// Le temps chronométré sur un lot un jour local, entrée ouverte comprise.
fn lot_day_total(ctx: &Ctx, lot_id: i64, date: &str) -> Result<i64> {
    let day: Interval = timeline::day_bounds(date, ctx.offset)?;
    let mut stmt = ctx.tx.prepare("SELECT start_at, end_at FROM entries WHERE removed = 0 AND lot_id = ?4 AND start_at < ?1 AND COALESCE(end_at, ?3) > ?2")?;
    let rows = stmt.query_map(params![day.end, day.start, ctx.now, lot_id], |r| {
        Ok((r.get::<_, i64>(0)?, r.get::<_, Option<i64>>(1)?))
    })?;
    let mut total = 0;
    for row in rows {
        let (start, end) = row?;
        let end = end.unwrap_or(ctx.now).min(day.end);
        total += (end - start.max(day.start)).max(0);
    }
    Ok(total)
}

// --- Journées ---------------------------------------------------------------

fn date_or_today(ctx: &Ctx, date: Option<&str>) -> Result<String> {
    match date {
        Some(d) => timeline::normalize_date(d),
        None => Ok(timeline::local_date(ctx.now, ctx.offset)),
    }
}

fn stored_status(ctx: &Ctx, date: &str) -> Result<DayStatus> {
    let text: Option<String> = ctx
        .tx
        .query_row("SELECT status FROM days WHERE date = ?1", [date], |r| {
            r.get(0)
        })
        .optional()?;
    Ok(text
        .as_deref()
        .and_then(DayStatus::parse)
        .unwrap_or(DayStatus::Open))
}

fn totals_json(totals: &Totals) -> String {
    serde_json::to_string(totals).expect("des totaux se sérialisent toujours")
}

fn day_row(ctx: &Ctx, date: &str) -> Result<Day> {
    let (closed_at, declared_at): (Option<i64>, Option<i64>) = ctx
        .tx
        .query_row(
            "SELECT closed_at, declared_at FROM days WHERE date = ?1",
            [date],
            |r| Ok((r.get(0)?, r.get(1)?)),
        )
        .optional()?
        .unwrap_or((None, None));
    Ok(Day {
        date: date.to_string(),
        status: stored_status(ctx, date)?,
        closed_at,
        declared_at,
        totals: totals(ctx, date)?,
    })
}

/// Une entrée ajoutée, modifiée ou supprimée sur une journée figée la fait réagir : une journée
/// saisie repasse « à resaisir » ; une journée clôturée reste clôturée mais ses totaux se rafraîchissent.
fn touch_day(ctx: &Ctx, date: &str) -> Result<()> {
    match stored_status(ctx, date)? {
        DayStatus::Declared => {
            ctx.tx.execute(
                "UPDATE days SET status = 'to_redeclare' WHERE date = ?1",
                [date],
            )?;
        }
        DayStatus::Closed => {
            let totals = totals(ctx, date)?;
            ctx.tx.execute(
                "UPDATE days SET closed_totals = ?1 WHERE date = ?2",
                params![totals_json(&totals), date],
            )?;
        }
        DayStatus::Open | DayStatus::ToRedeclare => {}
    }
    Ok(())
}

/// Les règles communes à un ajout et à une modification : de la durée, pas de futur, pas de minuit
/// traversé, pas de chevauchement. L'entrée `except` (celle qu'on modifie) ne compte pas contre elle-même.
fn check_range(ctx: &Ctx, from: i64, to: i64, except: Option<i64>) -> Result<()> {
    if to <= from {
        return Err(Error::EmptyEntry);
    }
    if to > ctx.now {
        return Err(Error::InFuture {
            at: fmt_hm(ctx, to),
            now: fmt_hm(ctx, ctx.now),
        });
    }
    if !timeline::midnights_between(from, to - 1, ctx.offset).is_empty() {
        return Err(Error::CrossesMidnight);
    }
    let clash: Option<(i64, Option<i64>, i64)> = ctx
        .tx
        .query_row(
            "SELECT start_at, end_at, lot_id FROM entries WHERE removed = 0 AND id != ?4 AND start_at < ?2 AND COALESCE(end_at, ?3) > ?1 ORDER BY start_at LIMIT 1",
            params![from, to, ctx.now, except.unwrap_or(-1)],
            |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
        )
        .optional()?;
    if let Some((start, end, lot_id)) = clash {
        return Err(Error::Overlap {
            with: format!(
                "{} – {} sur {}",
                fmt_hm(ctx, start),
                fmt_hm(ctx, end.unwrap_or(ctx.now)),
                lot_view(ctx, lot_id)?.target()
            ),
        });
    }
    Ok(())
}

/// Les entrées d'un jour local (une entrée ne traverse jamais minuit : son début suffit à la placer).
fn day_entries(ctx: &Ctx, date: &str) -> Result<Vec<(Entry, LotView)>> {
    let bounds = timeline::day_bounds(date, ctx.offset)?;
    let mut stmt = ctx.tx.prepare("SELECT id, lot_id, start_at, end_at, removed, note, source, workdir FROM entries WHERE start_at >= ?1 AND start_at < ?2 ORDER BY start_at, id")?;
    let entries = stmt
        .query_map([bounds.start, bounds.end], row_to_entry)?
        .collect::<std::result::Result<Vec<_>, _>>()?;
    entries
        .into_iter()
        .map(|e| Ok((e.clone(), lot_view(ctx, e.lot_id)?)))
        .collect()
}

fn day_view(ctx: &Ctx, date: &str) -> Result<DayView> {
    let rows = day_entries(ctx, date)?;
    let live: Vec<&Entry> = rows.iter().map(|(e, _)| e).filter(|e| !e.removed).collect();
    let end_of = |e: &Entry| e.end.unwrap_or(ctx.now).max(e.start);
    let start = live.first().map(|e| e.start);
    let end = live.last().map(|e| end_of(e));
    let pauses = live
        .windows(2)
        .map(|w| (w[1].start - end_of(w[0])).max(0))
        .sum();
    let total = live.iter().map(|e| end_of(e) - e.start).sum();
    let entries = rows
        .into_iter()
        .map(|(e, lot)| EntryView {
            id: e.id,
            lot,
            start: e.start,
            end: e.end,
            removed: e.removed,
        })
        .collect();
    Ok(DayView {
        date: date.to_string(),
        status: stored_status(ctx, date)?,
        entries,
        start,
        end,
        pauses,
        total,
    })
}

/// Les secondes par parent puis par sous-tâche (les lots sans ID Monday sont regroupés sous « sans ID »).
fn totals(ctx: &Ctx, date: &str) -> Result<Totals> {
    let mut totals = Totals::new();
    for (entry, lot) in day_entries(ctx, date)?
        .into_iter()
        .filter(|(e, _)| !e.removed)
    {
        let (parent, sub) = match &lot.parent_monday_id {
            Some(id) => (
                id.clone(),
                lot.subtask.clone().unwrap_or_else(|| "?".into()),
            ),
            None => (
                "sans ID".to_string(),
                lot.name
                    .clone()
                    .or(lot.subtask.clone())
                    .unwrap_or_else(|| "?".into()),
            ),
        };
        *totals.entry(parent).or_default().entry(sub).or_insert(0) +=
            (entry.end.unwrap_or(ctx.now) - entry.start).max(0);
    }
    Ok(totals)
}

fn check_setting_key(key: &str) -> Result<()> {
    match key {
        "main_subtask" | "rounding_minutes" | "claude_autonomy" | "menu_title_seconds" => Ok(()),
        other => Err(Error::InvalidSetting(format!(
            "clé inconnue « {other} » (main_subtask, rounding_minutes, claude_autonomy ou menu_title_seconds)"
        ))),
    }
}

/// La valeur d'un réglage qui n'a pas encore de ligne dans la base.
fn default_setting(key: &str) -> &'static str {
    match key {
        "menu_title_seconds" => "true",
        _ => "",
    }
}
