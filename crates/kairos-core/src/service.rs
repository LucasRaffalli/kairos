//! Le service : toutes les opérations de Kairos, chacune dans une transaction
//! `BEGIN IMMEDIATE`. Rien d'autre que ce module ne touche aux tables.

use rusqlite::{params, OptionalExtension, Transaction, TransactionBehavior};

use crate::clock::Clock;
use crate::db::Db;
use crate::error::{Error, Result};
use crate::model::{Entry, LotView, StartArgs, StartedLot, Status, StoppedEntry, SwitchResult};
use crate::timeline::{self, Interval};

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
            let entry_id = insert_open_entry(ctx, lot_id, ctx.now)?;
            clear_pause(ctx)?;
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
        self.with_tx(|ctx| {
            if let Some((_, lot_id, since)) = open_entry(ctx)? {
                let today = timeline::local_date(ctx.now, ctx.offset);
                return Ok(Status::Running {
                    lot: lot_view(ctx, lot_id)?,
                    since,
                    duration: ctx.now - since,
                    today_total: day_total(ctx, &today)?,
                });
            }
            if let Some(lot_id) = paused_lot(ctx)? {
                let since = last_entry_of(ctx, lot_id)?
                    .and_then(|e| e.end)
                    .unwrap_or(ctx.now);
                return Ok(Status::Paused {
                    lot: lot_view(ctx, lot_id)?,
                    since,
                });
            }
            Ok(Status::Idle)
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
        Some(monday_id) => {
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
                params![ctx.now, r.parent_label, id],
            )?;
            Some(id)
        }
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
    if let (Some(item), Some(id)) = (&r.subtask_monday_id, subtask_row) {
        ctx.tx.execute(
            "UPDATE subtasks SET monday_item_id = ?1 WHERE id = ?2",
            params![item, id],
        )?;
    }

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

/// Une sous-tâche de base du même nom est partagée ; sinon elle est propre au parent.
fn ensure_subtask(ctx: &Ctx, project: i64, parent: Option<i64>, name: &str) -> Result<i64> {
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
    ctx.tx.execute(
        "INSERT INTO subtasks (project_id, parent_id, name, is_base) VALUES (?1, ?2, ?3, 0)",
        params![project, parent, name],
    )?;
    Ok(ctx.tx.last_insert_rowid())
}

fn lot_view(ctx: &Ctx, lot_id: i64) -> Result<LotView> {
    Ok(ctx.tx.query_row(
        "SELECT l.id, p.monday_id, p.label, s.name, l.name FROM lots l LEFT JOIN parents p ON p.id = l.parent_id LEFT JOIN subtasks s ON s.id = l.subtask_id WHERE l.id = ?1",
        [lot_id],
        |row| Ok(LotView { id: row.get(0)?, parent_monday_id: row.get(1)?, parent_label: row.get(2)?, subtask: row.get(3)?, name: row.get(4)? }),
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
            "SELECT id, lot_id, start_at, end_at, removed, note FROM entries WHERE id = ?1",
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
    })
}

fn last_entry_of(ctx: &Ctx, lot_id: i64) -> Result<Option<Entry>> {
    Ok(ctx.tx.query_row("SELECT id, lot_id, start_at, end_at, removed, note FROM entries WHERE lot_id = ?1 ORDER BY start_at DESC LIMIT 1", [lot_id], row_to_entry).optional()?)
}

fn insert_open_entry(ctx: &Ctx, lot_id: i64, at: i64) -> Result<i64> {
    ctx.tx.execute(
        "INSERT INTO entries (lot_id, start_at) VALUES (?1, ?2)",
        params![lot_id, at],
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

/// Coupe l'entrée ouverte à chaque minuit local franchi : une entrée par jour, sur le même lot.
fn reconcile(ctx: &Ctx) -> Result<()> {
    let Some((id, lot_id, start)) = open_entry(ctx)? else {
        return Ok(());
    };
    let midnights = timeline::midnights_between(start, ctx.now, ctx.offset);
    let Some(&first) = midnights.first() else {
        return Ok(());
    };
    ctx.tx.execute(
        "UPDATE entries SET end_at = ?1 WHERE id = ?2",
        params![first, id],
    )?;
    for pair in midnights.windows(2) {
        ctx.tx.execute(
            "INSERT INTO entries (lot_id, start_at, end_at) VALUES (?1, ?2, ?3)",
            params![lot_id, pair[0], pair[1]],
        )?;
    }
    insert_open_entry(ctx, lot_id, *midnights.last().expect("liste non vide"))?;
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
    ensure_free_from(ctx, at)?;
    let lot_id = ensure_lot(ctx, &reference, args.source)?;
    let entry_id = insert_open_entry(ctx, lot_id, at)?;
    clear_pause(ctx)?;
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
        return Err(if explicit {
            Error::EmptyEntry
        } else {
            Error::ClockBehind {
                since: fmt_hm(ctx, start),
            }
        });
    }
    let lot = lot_view(ctx, lot_id)?;
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
