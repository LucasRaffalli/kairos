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
const COMMAND_MARKER: &str = "<!-- kairos:command";
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

/// Le programme entre guillemets simples : le shell n'y interprète rien (ni `$`, ni guillemet
/// inversé, ni espace).
fn shell_quote(program: &str) -> String {
    format!("'{}'", program.replace('\'', "'\\''"))
}

/// L'inverse de [`shell_quote`]. Reconnaît aussi l'ancienne forme, entre guillemets doubles.
fn shell_unquote(token: &str) -> String {
    match token.strip_prefix('\'').and_then(|t| t.strip_suffix('\'')) {
        Some(inner) => inner.replace("'\\''", "'"),
        None => token.trim_matches('"').to_string(),
    }
}

fn hook_command(program: &str, arg: &str) -> String {
    format!("{} hook {arg}", shell_quote(program))
}

/// Cette commande est-elle un hook de Kairos ? (`'…/kairos' hook session-start` ou `user-prompt`)
fn is_ours(command: &str) -> bool {
    EVENTS.iter().any(|(_, arg)| {
        command
            .strip_suffix(&format!(" hook {arg}"))
            .is_some_and(|program| {
                let program = shell_unquote(program.trim());
                Path::new(&program)
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
        let mut emptied_by_us = vec![false; entries.len()];
        for (i, entry) in entries.iter_mut().enumerate() {
            if let Some(Value::Array(handlers)) = entry.get_mut("hooks") {
                let before = handlers.len();
                handlers.retain(|h| !h["command"].as_str().is_some_and(is_ours));
                emptied_by_us[i] = handlers.len() != before && handlers.is_empty();
            }
        }
        if emptied_by_us.contains(&true) {
            let mut flags = emptied_by_us.into_iter();
            entries.retain(|_| !flags.next().unwrap_or(false));
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

/// Écrit un fichier sans jamais le laisser à moitié écrit. Un lien symbolique est suivi (on écrit
/// la cible, le lien reste un lien) et les droits du fichier d'origine sont conservés (un
/// `settings.json` en 0600 reste en 0600).
fn write_atomically(path: &Path, text: &str) -> Result<(), String> {
    let real = std::fs::canonicalize(path).unwrap_or_else(|_| path.to_path_buf());
    if let Some(dir) = real.parent() {
        std::fs::create_dir_all(dir).map_err(|e| format!("{} : {e}", dir.display()))?;
    }
    let name = real
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());
    let tmp = real.with_file_name(format!(".{name}.kairos-tmp{}", std::process::id()));
    std::fs::write(&tmp, text).map_err(|e| format!("{} : {e}", tmp.display()))?;
    if let Ok(meta) = std::fs::metadata(&real) {
        let _ = std::fs::set_permissions(&tmp, meta.permissions());
    }
    std::fs::rename(&tmp, &real).map_err(|e| {
        let _ = std::fs::remove_file(&tmp);
        format!("{} : {e}", real.display())
    })
}

/// Les fichiers de Claude Code que Kairos touche.
struct Paths {
    dir: PathBuf,
    settings: PathBuf,
    command: PathBuf,
}

impl Paths {
    fn locate() -> Result<Paths, String> {
        let dir = config_dir()?;
        Ok(Paths {
            settings: dir.join("settings.json"),
            command: dir.join("commands/kairos.md"),
            dir,
        })
    }

    /// Un chemin de sauvegarde qui n'existe pas encore (deux installations dans la même seconde
    /// ne s'écrasent pas).
    fn backup(&self) -> PathBuf {
        let stamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| d.as_secs());
        let base = format!("settings.json.kairos-backup-{stamp}");
        let mut candidate = self.dir.join(&base);
        let mut n = 2;
        while candidate.exists() {
            candidate = self.dir.join(format!("{base}-{n}"));
            n += 1;
        }
        candidate
    }
}

/// Où en est `commands/kairos.md`.
#[derive(Clone, Copy, PartialEq, Eq)]
enum CommandFile {
    Absent,
    /// Celui de Kairos, à jour.
    Current,
    /// Celui de Kairos, d'une version plus ancienne (reconnu à son repère).
    Outdated,
    /// Un fichier qui n'est pas celui de Kairos.
    Other,
}

impl CommandFile {
    fn inspect(path: &Path) -> CommandFile {
        match std::fs::read_to_string(path) {
            Ok(text) if text == COMMAND_FILE => CommandFile::Current,
            Ok(text) if text.contains(COMMAND_MARKER) => CommandFile::Outdated,
            Ok(_) => CommandFile::Other,
            Err(_) => CommandFile::Absent,
        }
    }
}

fn execute(action: &HooksAction) -> Result<String, String> {
    let paths = Paths::locate()?;
    let settings = read_settings(&paths.settings)?;
    let command_file = CommandFile::inspect(&paths.command);
    match action {
        HooksAction::Install { dry_run } => {
            install(&paths, settings.as_ref(), command_file, *dry_run)
        }
        HooksAction::Uninstall => uninstall(&paths, settings, command_file),
    }
}

/// Les réglages avec les hooks de Kairos en plus (les nôtres d'avant sont remplacés).
fn with_our_hooks(existing: Option<&Map<String, Value>>, program: &str) -> Map<String, Value> {
    let mut settings = existing.cloned().unwrap_or_default();
    remove_ours(&mut settings);
    let hooks = settings
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()))
        .as_object_mut()
        .expect("vérifié à la lecture");
    for (event, arg) in EVENTS {
        let handler = json!({
            "type": "command",
            "command": hook_command(program, arg),
            "timeout": HOOK_TIMEOUT,
        });
        hooks
            .entry(event)
            .or_insert_with(|| Value::Array(vec![]))
            .as_array_mut()
            .expect("vérifié à la lecture")
            .push(json!({ "hooks": [handler] }));
    }
    settings
}

fn to_text(settings: Map<String, Value>) -> Result<String, String> {
    serde_json::to_string_pretty(&Value::Object(settings))
        .map(|text| text + "\n")
        .map_err(|e| e.to_string())
}

fn install(
    paths: &Paths,
    existing: Option<&Map<String, Value>>,
    command_file: CommandFile,
    dry_run: bool,
) -> Result<String, String> {
    if command_file == CommandFile::Other {
        return Err(format!(
            "{} existe déjà et n'est pas celui de Kairos : déplace-le ou renomme-le, puis relance. Rien n'a été modifié.",
            paths.command.display()
        ));
    }
    let program = program()?;
    let updated = with_our_hooks(existing, &program);
    let settings_unchanged = existing == Some(&updated);
    if settings_unchanged && command_file == CommandFile::Current {
        return Ok(format!(
            "Déjà installé : les hooks Kairos sont dans {} (commande : {program}).",
            paths.settings.display()
        ));
    }
    if dry_run {
        let creates = if command_file == CommandFile::Absent {
            format!(", et créerait {}", paths.command.display())
        } else {
            String::new()
        };
        return Ok(format!(
            "Essai à blanc, rien n'est écrit. Kairos ajouterait ses hooks SessionStart et UserPromptSubmit dans {} (commande : {program}){creates}.",
            paths.settings.display()
        ));
    }

    let mut backup_note = String::new();
    if existing.is_some() && !settings_unchanged {
        let backup = paths.backup();
        std::fs::copy(&paths.settings, &backup)
            .map_err(|e| format!("sauvegarde impossible ({}) : {e}", backup.display()))?;
        backup_note = format!(" (sauvegarde : {})", backup.display());
    }
    if !settings_unchanged {
        write_atomically(&paths.settings, &to_text(updated)?)?;
    }
    write_atomically(&paths.command, COMMAND_FILE)?;
    Ok(format!(
        "✓ Hooks Kairos installés dans {} et commande /kairos créée{backup_note}.\nRedémarre Claude Code pour qu'il les prenne en compte.",
        paths.settings.display()
    ))
}

fn uninstall(
    paths: &Paths,
    existing: Option<Map<String, Value>>,
    command_file: CommandFile,
) -> Result<String, String> {
    let mut done = Vec::new();
    if let Some(settings) = existing {
        let mut cleaned = settings.clone();
        remove_ours(&mut cleaned);
        if cleaned != settings {
            write_atomically(&paths.settings, &to_text(cleaned)?)?;
            done.push("hooks retirés");
        }
    }
    match command_file {
        CommandFile::Current | CommandFile::Outdated => {
            std::fs::remove_file(&paths.command)
                .map_err(|e| format!("{} : {e}", paths.command.display()))?;
            done.push("commande /kairos retirée");
        }
        CommandFile::Other => {
            done.push("commande /kairos laissée (ce n'est pas le fichier de Kairos)");
        }
        CommandFile::Absent => {}
    }
    Ok(if done.is_empty() {
        "Rien à retirer : aucun hook de Kairos.".to_string()
    } else {
        format!("✓ {}.", done.join(", "))
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_program_path_survives_the_shell_whatever_it_contains() {
        for program in [
            "/Applications/Kairos.app/Contents/MacOS/kairos",
            "/tmp/un dossier/kairos",
            "/tmp/$HOME/`id`/kairos",
            "/tmp/guillemet\"/et'apostrophe/kairos",
            "/tmp/\\retour/kairos",
        ] {
            let command = hook_command(program, "user-prompt");
            // Le shell doit rendre le chemin tel quel, sans rien interpréter.
            let echoed = std::process::Command::new("sh")
                .arg("-c")
                .arg(format!(
                    "printf %s {}",
                    command.strip_suffix(" hook user-prompt").unwrap()
                ))
                .output()
                .unwrap();
            assert_eq!(String::from_utf8_lossy(&echoed.stdout), program);
            assert!(is_ours(&command), "{command}");
        }
    }

    #[test]
    fn hooks_written_by_an_earlier_version_are_still_recognised() {
        assert!(is_ours(
            "\"/Applications/Kairos.app/Contents/MacOS/kairos\" hook session-start"
        ));
    }

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
