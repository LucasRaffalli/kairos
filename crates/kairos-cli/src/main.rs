//! `kairos` : la ligne de commande. Aucune règle métier ici : elle traduit des arguments en
//! appels de `kairos-core` et rend le résultat en texte ou en JSON.

mod args;
mod hook;
mod mcp;
mod render;
mod time_input;

use args::{Cli, Command, LotArgs};
use clap::Parser;
use kairos_core::clock::{Clock, FakeClock, SystemClock};
use kairos_core::db::Db;
use kairos_core::error::{Error, Result};
use kairos_core::model::{AddEntry, EditEntry, Source, StartArgs};
use kairos_core::service::Kairos;
use kairos_core::timeline;
use render::Outcome;
use serde_json::json;

/// L'horloge du système, ou une horloge figée par `KAIROS_NOW` (secondes) et `KAIROS_TZ_OFFSET`
/// (secondes) : un crochet de test, lu ici seulement, jamais par le cœur.
pub(crate) enum AppClock {
    System(SystemClock),
    Fixed(FakeClock),
}

impl Clock for AppClock {
    fn now(&self) -> i64 {
        match self {
            AppClock::System(c) => c.now(),
            AppClock::Fixed(c) => c.now(),
        }
    }

    fn utc_offset(&self, at: i64) -> i32 {
        match self {
            AppClock::System(c) => c.utc_offset(at),
            AppClock::Fixed(c) => c.utc_offset(at),
        }
    }
}

pub(crate) fn app_clock() -> AppClock {
    match std::env::var("KAIROS_NOW")
        .ok()
        .and_then(|v| v.parse::<i64>().ok())
    {
        Some(now) => {
            let offset = std::env::var("KAIROS_TZ_OFFSET")
                .ok()
                .and_then(|v| v.parse::<i32>().ok())
                .unwrap_or(0);
            AppClock::Fixed(FakeClock::new(now, offset))
        }
        None => AppClock::System(SystemClock),
    }
}

fn main() {
    std::process::exit(run());
}

fn run() -> i32 {
    let cli = Cli::parse();
    if matches!(cli.command, Command::Version) {
        println!("kairos {}", kairos_core::version());
        return 0;
    }
    if let Command::Hook { event } = &cli.command {
        return hook::run(*event);
    }
    if let Command::Hooks { action } = &cli.command {
        return hook::install::run(action);
    }
    if matches!(cli.command, Command::Mcp) {
        return mcp::server::run();
    }
    let clock = app_clock();
    let outcome =
        Db::open(&Db::default_path()).and_then(|db| execute(&Kairos::new(db, clock), cli.command));
    match outcome {
        Ok((outcome, offset)) => {
            let offset = |at: i64| offset.utc_offset(at);
            if cli.json {
                println!("{}", render::to_json(&outcome));
            } else {
                println!("{}", render::text(&outcome, &offset));
            }
            0
        }
        Err(error) => {
            if cli.json {
                println!(
                    "{}",
                    json!({ "error": { "code": error.code(), "message": error.to_string() } })
                );
            } else {
                eprintln!("Erreur : {error}");
            }
            2
        }
    }
}

/// Le décalage local, gardé pour afficher les heures après l'exécution.
struct Offsets(AppClock);

impl Offsets {
    fn utc_offset(&self, at: i64) -> i32 {
        self.0.utc_offset(at)
    }
}

fn execute(k: &Kairos<AppClock>, command: Command) -> Result<(Outcome, Offsets)> {
    let offset = |at: i64| k.clock().utc_offset(at);
    let today = || timeline::local_date(k.clock().now(), &offset);
    let reference_date = |date: &Option<String>| date.clone().unwrap_or_else(today);
    let time = |input: &str, date: &str| time_input::parse(input, date, &offset);
    let optional_time =
        |input: &Option<String>, date: &str| input.as_deref().map(|t| time(t, date)).transpose();
    // Le dossier courant du processus : ce que la ligne de commande sait de l'extérieur.
    let current_dir = || {
        std::env::current_dir()
            .ok()
            .map(|dir| dir.to_string_lossy().into_owned())
    };
    let start_args = |a: LotArgs| -> Result<StartArgs> {
        Ok(StartArgs {
            at: optional_time(&a.at, &today())?,
            parent_monday_id: a.parent,
            parent_label: a.label,
            name: a.name,
            subtask: a.subtask,
            subtask_monday_id: a.subtask_id,
            source: a.source.into(),
            workdir: current_dir(),
        })
    };

    let outcome = match command {
        Command::Start(a) => Outcome::Started(k.start(start_args(a)?)?),
        Command::Stop { at } => Outcome::Stopped(k.stop(optional_time(&at, &today())?)?),
        Command::Pause => Outcome::Paused(k.pause()?),
        Command::Resume => Outcome::Resumed(k.resume()?),
        Command::Switch(a) => Outcome::Switched(Box::new(k.switch_to(start_args(a)?)?)),
        Command::Status => Outcome::Status(k.status()?),
        Command::Today { date } => Outcome::Today(k.today(date.as_deref())?),
        Command::Report { date } => Outcome::Report(k.report(date.as_deref())?),
        Command::CloseDay { date, stop } => Outcome::Closed(k.close_day(date.as_deref(), stop)?),
        Command::MarkDeclared { date } => Outcome::Declared(k.mark_declared(date.as_deref())?),
        Command::Add {
            parent,
            label,
            name,
            subtask,
            from,
            to,
            date,
        } => {
            let day = reference_date(&date);
            Outcome::EntryAdded(k.add_entry(AddEntry {
                parent_monday_id: parent,
                parent_label: label,
                name,
                subtask,
                from: time(&from, &day)?,
                to: time(&to, &day)?,
            })?)
        }
        Command::Edit {
            id,
            from,
            to,
            parent,
            name,
            subtask,
            date,
        } => {
            let day = reference_date(&date);
            Outcome::EntryEdited(k.edit_entry(
                id,
                EditEntry {
                    from: optional_time(&from, &day)?,
                    to: optional_time(&to, &day)?,
                    parent_monday_id: parent,
                    name,
                    subtask,
                },
            )?)
        }
        Command::Delete { id } => {
            k.delete_entry(id)?;
            Outcome::EntryDeleted(id)
        }
        Command::SetSubtaskId {
            parent,
            subtask,
            id,
        } => {
            k.set_subtask_id(&parent, &subtask, &id)?;
            Outcome::SubtaskId { parent, subtask }
        }
        Command::Recent { limit } => Outcome::Recent(k.recent_lots(limit)?),
        Command::Back => Outcome::Switched(Box::new(k.back(Source::Manual, current_dir())?)),
        Command::Label { id, name } => {
            let label = k.set_label(&id, &name)?;
            Outcome::Label { parent: id, label }
        }
        Command::Config { key, value } => {
            if let Some(value) = &value {
                k.config_set(&key, value)?;
            }
            let value = k.config_get(&key)?;
            Outcome::Config { key, value }
        }
        Command::Version | Command::Mcp | Command::Hook { .. } | Command::Hooks { .. } => {
            return Err(Error::Internal(
                "`version`, `mcp` et `hook` ne passent pas par le service".into(),
            ))
        }
    };
    Ok((outcome, Offsets(app_clock())))
}
