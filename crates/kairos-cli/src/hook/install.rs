//! `kairos hooks install` / `uninstall` : écrit (ou retire) les hooks de Kairos dans les réglages
//! de Claude Code, et la commande `/kairos`.
//!
//! On ne touche à rien d'autre : les autres hooks, les permissions et l'ordre des clés restent
//! comme ils étaient. Avant d'écrire, tout est vérifié (JSON valide, commande `/kairos` libre) ;
//! un fichier de réglages est sauvegardé avant d'être modifié.

use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use serde_json::{json, Map, Value};

use crate::args::HooksAction;

const COMMAND_FILE: &str = include_str!("kairos_command.md");
/// Délai accordé à un hook par Claude Code, en secondes.
const HOOK_TIMEOUT: u64 = 5;
/// (événement de Claude Code, argument de `kairos hook`)
const EVENTS: [(&str, &str); 2] = [
    ("SessionStart", "session-start"),
    ("UserPromptSubmit", "user-prompt"),
];

pub fn run(action: &HooksAction) -> i32 {
    match execute(action) {
        Ok(text) => {
            println!("{text}");
            0
        }
        Err(why) => {
            eprintln!("Erreur : {why}");
            1
        }
    }
}

fn config_dir() -> Result<PathBuf, String> {
    if let Some(dir) = std::env::var_os("CLAUDE_CONFIG_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = std::env::var_os("HOME")
        .ok_or("HOME n'est pas défini : impossible de trouver ~/.claude")?;
    Ok(PathBuf::from(home).join(".claude"))
}

/// Le chemin absolu du binaire courant : les hooks ne dépendent pas du PATH de l'application
/// qui lance Claude Code.
fn program() -> Result<String, String> {
    let exe = std::env::current_exe().map_err(|e| e.to_string())?;
    let exe = std::fs::canonicalize(&exe).unwrap_or(exe);
    Ok(exe.to_string_lossy().to_string())
}

fn hook_command(program: &str, arg: &str) -> String {
    format!("\"{program}\" hook {arg}")
}

/// Cette commande est-elle un hook de Kairos ? (`… kairos" hook session-start` ou `user-prompt`)
fn is_ours(command: &str) -> bool {
    EVENTS.iter().any(|(_, arg)| {
        command
            .strip_suffix(&format!(" hook {arg}"))
            .is_some_and(|program| {
                let program = program.trim().trim_matches('"');
                Path::new(program)
                    .file_name()
                    .is_some_and(|n| n == "kairos")
            })
    })
}

fn read_settings(path: &Path) -> Result<Option<Map<String, Value>>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(format!("{} illisible : {e}", path.display())),
    };
    if text.trim().is_empty() {
        return Ok(None);
    }
    let invalid = |why: &str| {
        format!(
            "{} n'est pas un réglage valide ({why}) : corrige-le à la main, rien n'a été modifié.",
            path.display()
        )
    };
    match serde_json::from_str::<Value>(&text) {
        Ok(Value::Object(map)) => {
            match map.get("hooks") {
                None | Some(Value::Object(_)) => {}
                Some(_) => return Err(invalid("« hooks » doit être un objet")),
            }
            if let Some(Value::Object(hooks)) = map.get("hooks") {
                for (event, entries) in hooks {
                    if !entries.is_array() {
                        return Err(invalid(&format!("« hooks.{event} » doit être une liste")));
                    }
                }
            }
            Ok(Some(map))
        }
        Ok(_) => Err(invalid("la racine doit être un objet")),
        Err(e) => Err(invalid(&e.to_string())),
    }
}

/// Retire les hooks de Kairos de `hooks.<événement>` ; une entrée qui n'avait que les nôtres et un
/// événement vidé disparaissent, rien d'autre.
fn remove_ours(settings: &mut Map<String, Value>) {
    let Some(Value::Object(hooks)) = settings.get_mut("hooks") else {
        return;
    };
    let events: Vec<String> = hooks.keys().cloned().collect();
    for event in events {
        let Some(Value::Array(entries)) = hooks.get_mut(&event) else {
            continue;
        };
        let mut touched = false;
        for entry in entries.iter_mut() {
            if let Some(Value::Array(handlers)) = entry.get_mut("hooks") {
                let before = handlers.len();
                handlers.retain(|h| !h["command"].as_str().is_some_and(is_ours));
                touched |= handlers.len() != before;
            }
        }
        if touched {
            entries.retain(|e| e["hooks"].as_array().is_none_or(|h| !h.is_empty()));
            if entries.is_empty() {
                hooks.shift_remove(&event);
            }
        }
    }
    if hooks.is_empty()
        && settings
            .get("hooks")
            .is_some_and(|h| h.as_object().is_some_and(Map::is_empty))
    {
        // « hooks » n'existait pas forcément avant nous : on ne laisse pas un objet vide.
        settings.shift_remove("hooks");
    }
}

fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    if let Some(dir) = path.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    }
    let tmp = path.with_extension("kairos-tmp");
    std::fs::write(&tmp, text).map_err(|e| format!("{} : {e}", tmp.display()))?;
    std::fs::rename(&tmp, path).map_err(|e| format!("{} : {e}", path.display()))
}

fn execute(action: &HooksAction) -> Result<String, String> {
    let dir = config_dir()?;
    let settings_path = dir.join("settings.json");
    let command_path = dir.join("commands/kairos.md");
    let existing = read_settings(&settings_path)?;
    let command_state = match std::fs::read_to_string(&command_path) {
        Ok(text) if text == COMMAND_FILE => CommandFile::Ours,
        Ok(_) => CommandFile::Other,
        Err(_) => CommandFile::Absent,
    };

    match action {
        HooksAction::Install { dry_run } => {
            if command_state == CommandFile::Other {
                return Err(format!(
                    "{} existe déjà et n'est pas celui de Kairos : déplace-le ou renomme-le, puis relance. Rien n'a été modifié.",
                    command_path.display()
                ));
            }
            let program = program()?;
            let mut settings = existing.clone().unwrap_or_default();
            remove_ours(&mut settings);
            let hooks = settings
                .entry("hooks")
                .or_insert_with(|| Value::Object(Map::new()))
                .as_object_mut()
                .expect("vérifié à la lecture");
            for (event, arg) in EVENTS {
                let handler = json!({
                    "type": "command",
                    "command": hook_command(&program, arg),
                    "timeout": HOOK_TIMEOUT,
                });
                hooks
                    .entry(event)
                    .or_insert_with(|| Value::Array(vec![]))
                    .as_array_mut()
                    .expect("vérifié à la lecture")
                    .push(json!({ "hooks": [handler] }));
            }
            if *dry_run {
                return Ok(format!(
                    "Essai à blanc, rien n'est écrit. Kairos ajouterait ses hooks SessionStart et UserPromptSubmit dans {} (commande : {program}){}.",
                    settings_path.display(),
                    if command_state == CommandFile::Absent {
                        format!(", et créerait {}", command_path.display())
                    } else {
                        String::new()
                    }
                ));
            }
            let mut report = Vec::new();
            if existing.is_some() {
                let stamp = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .map_or(0, |d| d.as_secs());
                let backup = dir.join(format!("settings.json.kairos-backup-{stamp}"));
                std::fs::copy(&settings_path, &backup)
                    .map_err(|e| format!("sauvegarde impossible ({}) : {e}", backup.display()))?;
                report.push(format!("sauvegarde : {}", backup.display()));
            }
            let text = serde_json::to_string_pretty(&Value::Object(settings))
                .map_err(|e| e.to_string())?;
            write_atomically(&settings_path, &(text + "\n"))?;
            write_atomically(&command_path, COMMAND_FILE)?;
            Ok(format!(
                "✓ Hooks Kairos installés dans {} et commande /kairos créée{}.\nRedémarre Claude Code pour qu'il les prenne en compte.",
                settings_path.display(),
                if report.is_empty() { String::new() } else { format!(" ({})", report.join(", ")) }
            ))
        }
        HooksAction::Uninstall => {
            let mut notes = Vec::new();
            if let Some(mut settings) = existing {
                let before = settings.clone();
                remove_ours(&mut settings);
                if settings != before {
                    let text = serde_json::to_string_pretty(&Value::Object(settings))
                        .map_err(|e| e.to_string())?;
                    write_atomically(&settings_path, &(text + "\n"))?;
                    notes.push("hooks retirés");
                }
            }
            match command_state {
                CommandFile::Ours => {
                    std::fs::remove_file(&command_path)
                        .map_err(|e| format!("{} : {e}", command_path.display()))?;
                    notes.push("commande /kairos retirée");
                }
                CommandFile::Other => {
                    notes.push("commande /kairos laissée (ce n'est plus le fichier de Kairos)");
                }
                CommandFile::Absent => {}
            }
            Ok(if notes.is_empty() {
                "Rien à retirer : aucun hook de Kairos.".to_string()
            } else {
                format!("✓ {}.", notes.join(", "))
            })
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandFile {
    Absent,
    Ours,
    Other,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_kairos_hook_commands_are_ours() {
        assert!(is_ours(
            "\"/Applications/Kairos.app/Contents/MacOS/kairos\" hook user-prompt"
        ));
        assert!(is_ours("\"/opt/homebrew/bin/kairos\" hook session-start"));
        assert!(!is_ours("/usr/local/bin/autre-outil prompt"));
        assert!(!is_ours("\"/usr/local/bin/autre\" hook user-prompt"));
        assert!(!is_ours("kairos hook autre-chose"));
    }
}
