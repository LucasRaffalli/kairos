//! `kairos hooks install` / `uninstall` : la configuration de Claude Code dans un faux dossier.

use std::path::PathBuf;
use std::process::{Command, Output};

use serde_json::{json, Value};

struct Home {
    dir: tempfile::TempDir,
}

impl Home {
    fn new() -> Home {
        Home {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn settings(&self) -> PathBuf {
        self.dir.path().join("settings.json")
    }

    fn command_file(&self) -> PathBuf {
        self.dir.path().join("commands/kairos.md")
    }

    fn run(&self, args: &[&str]) -> Output {
        Command::new(env!("CARGO_BIN_EXE_kairos"))
            .args(args)
            .env("CLAUDE_CONFIG_DIR", self.dir.path())
            .env("HOME", self.dir.path().join("home-sans-rien"))
            .output()
            .unwrap()
    }

    fn read_settings(&self) -> Value {
        serde_json::from_str(&std::fs::read_to_string(self.settings()).unwrap()).unwrap()
    }

    fn backups(&self) -> Vec<PathBuf> {
        let mut found: Vec<PathBuf> = std::fs::read_dir(self.dir.path())
            .unwrap()
            .map(|e| e.unwrap().path())
            .filter(|p| {
                p.file_name()
                    .unwrap()
                    .to_string_lossy()
                    .starts_with("settings.json.kairos-backup-")
            })
            .collect();
        found.sort();
        found
    }
}

fn exe() -> String {
    std::fs::canonicalize(env!("CARGO_BIN_EXE_kairos"))
        .unwrap()
        .to_string_lossy()
        .to_string()
}

fn code(o: &Output) -> Option<i32> {
    o.status.code()
}

fn stderr(o: &Output) -> String {
    String::from_utf8_lossy(&o.stderr).to_string()
}

fn stdout(o: &Output) -> String {
    String::from_utf8_lossy(&o.stdout).to_string()
}

/// Les commandes de hook de Kairos rangées sous cet événement.
fn ours(settings: &Value, event: &str) -> Vec<String> {
    settings["hooks"][event]
        .as_array()
        .map(|entries| {
            entries
                .iter()
                .flat_map(|e| e["hooks"].as_array().cloned().unwrap_or_default())
                .filter_map(|h| h["command"].as_str().map(String::from))
                .filter(|c| c.contains("kairos") && c.contains(" hook "))
                .collect()
        })
        .unwrap_or_default()
}

#[test]
fn install_creates_settings_and_command_when_absent() {
    let h = Home::new();
    let out = h.run(&["hooks", "install"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let settings = h.read_settings();
    assert_eq!(
        ours(&settings, "SessionStart"),
        [format!("\"{}\" hook session-start", exe())]
    );
    assert_eq!(
        ours(&settings, "UserPromptSubmit"),
        [format!("\"{}\" hook user-prompt", exe())]
    );
    let handler = &settings["hooks"]["UserPromptSubmit"][0]["hooks"][0];
    assert_eq!(handler["type"], "command");
    assert_eq!(handler["timeout"], 5);
    let command = std::fs::read_to_string(h.command_file()).unwrap();
    assert!(command.contains("$ARGUMENTS"), "{command}");
    assert!(h.backups().is_empty(), "rien à sauvegarder");
}

#[test]
fn the_command_file_mentions_status_recent_and_switch() {
    let h = Home::new();
    h.run(&["hooks", "install"]);
    let command = std::fs::read_to_string(h.command_file()).unwrap();
    for word in ["status", "recent", "switch", "Ne devine jamais"] {
        assert!(command.contains(word), "{word} : {command}");
    }
}

#[test]
fn install_keeps_existing_settings_and_other_hooks_and_makes_a_backup() {
    let h = Home::new();
    let original = json!({
        "permissions": { "allow": ["Bash(ls:*)"] },
        "model": "opus",
        "hooks": {
            "PreToolUse": [{ "matcher": "Bash", "hooks": [{ "type": "command", "command": "/usr/local/bin/audit" }] }],
            "UserPromptSubmit": [{ "hooks": [{ "type": "command", "command": "/usr/local/bin/autre-outil prompt" }] }],
        },
    });
    std::fs::write(
        h.settings(),
        serde_json::to_string_pretty(&original).unwrap(),
    )
    .unwrap();
    let out = h.run(&["hooks", "install"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let settings = h.read_settings();
    assert_eq!(settings["permissions"], original["permissions"]);
    assert_eq!(settings["model"], "opus");
    assert_eq!(
        settings["hooks"]["PreToolUse"],
        original["hooks"]["PreToolUse"]
    );
    let prompts = settings["hooks"]["UserPromptSubmit"].as_array().unwrap();
    assert_eq!(prompts[0], original["hooks"]["UserPromptSubmit"][0]);
    assert_eq!(ours(&settings, "UserPromptSubmit").len(), 1);
    // L'ordre des clés de l'utilisateur est conservé.
    let keys: Vec<_> = settings.as_object().unwrap().keys().cloned().collect();
    assert_eq!(keys, ["permissions", "model", "hooks"]);
    let backups = h.backups();
    assert_eq!(backups.len(), 1);
    let saved: Value =
        serde_json::from_str(&std::fs::read_to_string(&backups[0]).unwrap()).unwrap();
    assert_eq!(saved, original);
}

#[test]
fn install_twice_does_not_duplicate_and_updates_the_program_path() {
    let h = Home::new();
    h.run(&["hooks", "install"]);
    let mut settings = h.read_settings();
    // Un ancien emplacement du programme (par exemple avant un changement d'installation).
    settings["hooks"]["SessionStart"][0]["hooks"][0]["command"] =
        json!("\"/ancien/chemin/kairos\" hook session-start");
    std::fs::write(h.settings(), settings.to_string()).unwrap();
    assert_eq!(code(&h.run(&["hooks", "install"])), Some(0));
    assert_eq!(code(&h.run(&["hooks", "install"])), Some(0));
    let settings = h.read_settings();
    assert_eq!(
        ours(&settings, "SessionStart"),
        [format!("\"{}\" hook session-start", exe())]
    );
    assert_eq!(ours(&settings, "UserPromptSubmit").len(), 1);
}

#[test]
fn dry_run_writes_nothing_and_says_what_it_would_do() {
    let h = Home::new();
    let out = h.run(&["hooks", "install", "--dry-run"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    assert!(!h.settings().exists() && !h.command_file().exists());
    let text = stdout(&out);
    assert!(
        text.contains("settings.json")
            && text.contains("SessionStart")
            && text.contains("UserPromptSubmit"),
        "{text}"
    );
}

#[test]
fn invalid_settings_json_is_refused_and_nothing_is_touched() {
    let h = Home::new();
    std::fs::write(h.settings(), "{ \"hooks\": ").unwrap();
    let out = h.run(&["hooks", "install"]);
    assert_eq!(code(&out), Some(1));
    assert!(stderr(&out).contains("settings.json"), "{}", stderr(&out));
    assert_eq!(
        std::fs::read_to_string(h.settings()).unwrap(),
        "{ \"hooks\": "
    );
    assert!(!h.command_file().exists());
    assert!(h.backups().is_empty());
    for odd in [
        "[1, 2]",
        r#"{"hooks": 3}"#,
        r#"{"hooks": {"UserPromptSubmit": "x"}}"#,
    ] {
        std::fs::write(h.settings(), odd).unwrap();
        assert_eq!(code(&h.run(&["hooks", "install"])), Some(1), "{odd}");
        assert_eq!(std::fs::read_to_string(h.settings()).unwrap(), odd);
    }
}

#[test]
fn an_empty_settings_file_is_rewritten() {
    let h = Home::new();
    std::fs::write(h.settings(), "  \n").unwrap();
    assert_eq!(code(&h.run(&["hooks", "install"])), Some(0));
    assert_eq!(ours(&h.read_settings(), "SessionStart").len(), 1);
}

#[test]
fn a_different_kairos_command_file_is_not_overwritten() {
    let h = Home::new();
    std::fs::create_dir_all(h.command_file().parent().unwrap()).unwrap();
    std::fs::write(h.command_file(), "ma propre commande").unwrap();
    std::fs::write(h.settings(), r#"{"model":"opus"}"#).unwrap();
    let out = h.run(&["hooks", "install"]);
    assert_eq!(code(&out), Some(1));
    assert!(
        stderr(&out).contains("commands/kairos.md"),
        "{}",
        stderr(&out)
    );
    assert_eq!(
        std::fs::read_to_string(h.command_file()).unwrap(),
        "ma propre commande"
    );
    assert_eq!(
        std::fs::read_to_string(h.settings()).unwrap(),
        r#"{"model":"opus"}"#
    );
}

#[test]
fn uninstall_removes_only_kairos_hooks_and_the_command() {
    let h = Home::new();
    let other = json!({ "type": "command", "command": "/usr/local/bin/autre-outil prompt" });
    std::fs::write(
        h.settings(),
        json!({ "model": "opus", "hooks": { "UserPromptSubmit": [{ "hooks": [other.clone()] }] } })
            .to_string(),
    )
    .unwrap();
    h.run(&["hooks", "install"]);
    let out = h.run(&["hooks", "uninstall"]);
    assert_eq!(code(&out), Some(0), "{}", stderr(&out));
    let settings = h.read_settings();
    assert_eq!(settings["model"], "opus");
    assert_eq!(
        settings["hooks"]["UserPromptSubmit"],
        json!([{ "hooks": [other] }])
    );
    assert!(
        settings["hooks"].get("SessionStart").is_none(),
        "l'événement vidé disparaît"
    );
    assert!(ours(&settings, "UserPromptSubmit").is_empty());
    assert!(!h.command_file().exists());
    // Relancée, elle ne fait rien et sort en 0.
    assert_eq!(code(&h.run(&["hooks", "uninstall"])), Some(0));
}

#[test]
fn uninstall_leaves_a_command_file_that_is_not_ours() {
    let h = Home::new();
    h.run(&["hooks", "install"]);
    std::fs::write(h.command_file(), "modifiée par l'utilisateur").unwrap();
    assert_eq!(code(&h.run(&["hooks", "uninstall"])), Some(0));
    assert!(h.command_file().exists());
}

#[test]
fn uninstall_without_any_settings_is_a_no_op() {
    let h = Home::new();
    assert_eq!(code(&h.run(&["hooks", "uninstall"])), Some(0));
    assert!(!h.settings().exists());
}

#[cfg(unix)]
#[test]
fn a_symlinked_settings_file_stays_a_symlink_and_keeps_its_permissions() {
    use std::os::unix::fs::{symlink, PermissionsExt};
    let h = Home::new();
    let dotfiles = tempfile::tempdir().unwrap();
    let target = dotfiles.path().join("claude-settings.json");
    std::fs::write(&target, r#"{"env":{"CLE":"secrète"}}"#).unwrap();
    std::fs::set_permissions(&target, std::fs::Permissions::from_mode(0o600)).unwrap();
    symlink(&target, h.settings()).unwrap();
    assert_eq!(code(&h.run(&["hooks", "install"])), Some(0));
    assert!(std::fs::symlink_metadata(h.settings())
        .unwrap()
        .file_type()
        .is_symlink());
    let written: Value = serde_json::from_str(&std::fs::read_to_string(&target).unwrap()).unwrap();
    assert_eq!(written["env"]["CLE"], "secrète");
    assert_eq!(ours(&written, "SessionStart").len(), 1);
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // Un fichier ordinaire en 0600 le reste aussi, à la désinstallation.
    assert_eq!(code(&h.run(&["hooks", "uninstall"])), Some(0));
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
}

#[test]
fn an_older_kairos_command_file_is_updated_and_an_edited_one_is_not_ours() {
    let h = Home::new();
    std::fs::create_dir_all(h.command_file().parent().unwrap()).unwrap();
    // Un fichier d'une ancienne version de Kairos : reconnu à son repère, donc mis à jour.
    std::fs::write(
        h.command_file(),
        "<!-- kairos:command -->\nancienne version",
    )
    .unwrap();
    assert_eq!(code(&h.run(&["hooks", "install"])), Some(0));
    let now = std::fs::read_to_string(h.command_file()).unwrap();
    assert!(
        now.contains("$ARGUMENTS") && now.contains("<!-- kairos:command"),
        "{now}"
    );
    // Désinstallation : le fichier marqué est retiré, même d'une ancienne version.
    std::fs::write(
        h.command_file(),
        "<!-- kairos:command -->\nencore une autre",
    )
    .unwrap();
    assert_eq!(code(&h.run(&["hooks", "uninstall"])), Some(0));
    assert!(!h.command_file().exists());
}

#[test]
fn uninstall_keeps_an_empty_entry_the_user_had_before() {
    let h = Home::new();
    std::fs::write(
        h.settings(),
        json!({ "hooks": { "UserPromptSubmit": [{ "hooks": [] }] } }).to_string(),
    )
    .unwrap();
    h.run(&["hooks", "install"]);
    h.run(&["hooks", "uninstall"]);
    assert_eq!(
        h.read_settings()["hooks"]["UserPromptSubmit"],
        json!([{ "hooks": [] }])
    );
}
