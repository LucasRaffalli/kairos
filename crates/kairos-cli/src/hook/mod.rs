//! `kairos hook <événement>` : les hooks de Claude Code. Le JSON de l'événement arrive sur
//! l'entrée standard, la réponse (un JSON, ou rien) part sur la sortie standard.
//!
//! **Un hook ne fait jamais échouer le message de l'utilisateur** : quoi qu'il arrive (entrée
//! invalide, base verrouillée ou plus récente, Git absent, panique), le code de sortie est 0 et
//! le détail part sur la sortie d'erreur. Le texte du message n'est lu que pour y chercher des
//! IDs, puis jeté.

mod input;
pub mod install;
mod output;
mod rules;
mod sources;
mod state;

use std::path::Path;

use kairos_core::clock::Clock;
use kairos_core::db::Db;
use kairos_core::detect::{self, Autonomy, DecideInput, Decision, FoundId, Origin, Plan, Resolved};
use kairos_core::error::Result;
use kairos_core::model::{KnownId, LotView, Source, StartArgs, Status};
use kairos_core::service::Kairos;

use crate::args::HookEvent;
use crate::render::{self, Outcome};

/// Ce que le hook rend : de quoi informer Claude, de quoi informer l'utilisateur.
#[derive(Default)]
struct Reply {
    context: Option<String>,
    message: Option<String>,
}

pub fn run(event: HookEvent) -> i32 {
    let outcome = std::panic::catch_unwind(|| try_run(event));
    match outcome {
        Ok(Ok(Some(json))) => println!("{json}"),
        Ok(Ok(None)) => {}
        Ok(Err(error)) => eprintln!("kairos hook : {error}"),
        Err(_) => eprintln!("kairos hook : erreur interne, ignorée"),
    }
    0
}

fn try_run(event: HookEvent) -> Result<Option<String>> {
    let input = input::read(std::io::stdin().lock());
    let k = Kairos::new(Db::open(&Db::default_path())?, crate::app_clock());
    let (name, reply) = match event {
        HookEvent::SessionStart => ("SessionStart", session_start(&k)?),
        HookEvent::UserPrompt => ("UserPromptSubmit", user_prompt(&k, &input)?),
    };
    Ok(output::render(name, reply.context, reply.message))
}

// --- SessionStart -------------------------------------------------------------------------------

fn session_start<C: Clock>(k: &Kairos<C>) -> Result<Reply> {
    let status = k.status()?;
    let line = match status {
        Status::Idle => "Kairos : aucun lot en cours.".to_string(),
        other => {
            let offset = |at: i64| k.clock().utc_offset(at);
            format!(
                "Kairos : {}",
                render::text(&Outcome::Status(other), &offset)
            )
        }
    };
    Ok(Reply {
        context: Some(format!("{line}\n\n{}", rules::SESSION_RULES)),
        message: None,
    })
}

// --- UserPromptSubmit ---------------------------------------------------------------------------

fn user_prompt<C: Clock>(k: &Kairos<C>, input: &input::HookInput) -> Result<Reply> {
    let running_parent = match k.status()? {
        Status::Running { lot, .. } | Status::Paused { lot, .. } => lot.parent_monday_id,
        Status::Idle => None,
    };
    let autonomy = if k.config_get("claude_autonomy")? == "auto" {
        Autonomy::Auto
    } else {
        Autonomy::Ask
    };

    let message = resolve_all(k, detect::extract_ids(&input.prompt));
    let mut decide_input = DecideInput {
        running_parent,
        autonomy,
        message,
        branch_or_commits: vec![],
        conflicting_branch_and_commits: false,
        config: None,
    };
    // Git et le fichier ne sont lus que si le message ne dit rien : un ID explicite l'emporte.
    if decide_input.message.is_empty() {
        if let Some(cwd) = input.cwd.as_deref() {
            read_workspace(k, cwd, &mut decide_input);
        }
    }

    Ok(match detect::decide(&decide_input) {
        Decision::Nothing => Reply::default(),
        Decision::Doubt(text) => once(input, &text, text.clone()),
        Decision::Inform(plan) => {
            let text = inform_text(&plan);
            let key = format!("inform:{}:{:?}", plan.parent, decide_input.running_parent);
            once(input, &key, text)
        }
        Decision::Act(plan) => act(k, input, &plan),
    })
}

/// Le texte `text` pour Claude, une seule fois par session pour cette `key`.
fn once(input: &input::HookInput, key: &str, text: String) -> Reply {
    if state::already_asked(&input.session_id, key) {
        return Reply::default();
    }
    state::mark_asked(&input.session_id, key);
    Reply {
        context: Some(text),
        message: None,
    }
}

fn read_workspace<C: Clock>(k: &Kairos<C>, cwd: &Path, decide_input: &mut DecideInput) {
    let branch = sources::git_branch(cwd)
        .map(|b| resolve_all(k, detect::extract_ids(&b)))
        .unwrap_or_default();
    let commits = sources::git_commits(cwd)
        .map(|c| resolve_all(k, detect::extract_ids(&c)))
        .unwrap_or_default();
    decide_input.conflicting_branch_and_commits = !branch.is_empty()
        && !commits.is_empty()
        && !branch
            .iter()
            .any(|b| commits.iter().any(|c| c.parent == b.parent));
    // Du commit le plus récent au plus ancien, puis ce que seule la branche cite.
    let mut set = commits;
    for b in branch {
        if !set.iter().any(|s| s.parent == b.parent) {
            set.push(b);
        }
    }
    decide_input.branch_or_commits = set;

    if let Some(config) = sources::kairos_json(cwd) {
        let new_parent = matches!(k.resolve_monday_id(&config.parent), Ok(KnownId::Unknown));
        decide_input.config = Some(Resolved {
            parent: config.parent,
            subtask: config.subtask,
            label: config.label,
            sure: true,
            new_parent,
        });
    }
}

/// Résout chaque ID auprès de Kairos : un parent connu, la sous-tâche mémorisée d'un parent (ramenée
/// à son parent), ou un ID inconnu. Un seul `Resolved` par parent.
fn resolve_all<C: Clock>(k: &Kairos<C>, found: Vec<FoundId>) -> Vec<Resolved> {
    let mut out: Vec<Resolved> = Vec::new();
    for f in found {
        let Ok(known) = k.resolve_monday_id(&f.id) else {
            continue;
        };
        let resolved = match known {
            KnownId::Parent { label } => Resolved {
                parent: f.id,
                subtask: None,
                label,
                sure: true,
                new_parent: false,
            },
            KnownId::Subtask {
                parent_monday_id,
                subtask,
                label,
            } => Resolved {
                parent: parent_monday_id,
                subtask: Some(subtask),
                label,
                sure: true,
                new_parent: false,
            },
            KnownId::Unknown => Resolved {
                parent: f.id,
                subtask: None,
                label: None,
                sure: f.sure_by_shape,
                new_parent: true,
            },
        };
        match out.iter_mut().find(|o| o.parent == resolved.parent) {
            Some(existing) => {
                existing.sure |= resolved.sure;
                existing.subtask = existing.subtask.take().or(resolved.subtask);
            }
            None => out.push(resolved),
        }
    }
    out
}

fn origin_text(origin: Origin) -> &'static str {
    match origin {
        Origin::Message => "dans le message",
        Origin::BranchOrCommits => "dans la branche ou les commits",
        Origin::Config => "dans .kairos.json",
    }
}

fn named(lot: &LotView) -> String {
    match &lot.parent_label {
        Some(label) => format!("{label} — {}", lot.describe()),
        None => lot.describe(),
    }
}

fn inform_text(plan: &Plan) -> String {
    let unknown = if plan.new_parent {
        ", parent inconnu de Kairos"
    } else {
        ""
    };
    format!(
        "Kairos : ID détecté {} ({}{unknown}). Appelle `switch` (ou `start`) si c'est bien ça.",
        plan.parent,
        origin_text(plan.origin)
    )
}

fn act<C: Clock>(k: &Kairos<C>, input: &input::HookInput, plan: &Plan) -> Reply {
    let result = k.switch_to(StartArgs {
        parent_monday_id: Some(plan.parent.clone()),
        parent_label: plan.label.clone(),
        subtask: plan.subtask.clone(),
        source: Source::ClaudeCode,
        ..StartArgs::default()
    });
    let switched = match result {
        Ok(switched) => switched,
        Err(error) => {
            eprintln!("kairos hook : {error}");
            return Reply::default();
        }
    };
    let new_parent = if plan.new_parent {
        " (nouveau parent)"
    } else {
        ""
    };
    let now = named(&switched.started.lot);
    let message = match &switched.stopped {
        Some(stopped) => {
            let back = match &stopped.lot.parent_monday_id {
                Some(id) => format!("kairos back (ou kairos switch --parent {id})"),
                None => "kairos back".to_string(),
            };
            format!(
                "Kairos : basculé vers {now}{new_parent} (avant : {}). Pour revenir : {back}.",
                named(&stopped.lot)
            )
        }
        None => format!("Kairos : lot lancé sur {now}{new_parent}."),
    };
    let mut context = message.clone();
    let hint_key = format!("label:{}", plan.parent);
    if switched.started.lot.parent_label.is_none()
        && !state::already_asked(&input.session_id, &hint_key)
    {
        state::mark_asked(&input.session_id, &hint_key);
        context.push_str(&format!(
            " Lot lancé sans nom : si tu connais le nom de la tâche, enregistre-le avec `set_label` (parent {}).",
            plan.parent
        ));
    }
    Reply {
        context: Some(context),
        message: Some(message),
    }
}
