//! Ce que le hook va lire à l'extérieur : Git et `.kairos.json`. Un échec vaut « pas de source ».

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde::Deserialize;

const GIT_TIMEOUT: Duration = Duration::from_secs(2);

/// Lance Git dans `cwd` ; `None` s'il échoue, s'il n'existe pas ou s'il dépasse 2 secondes.
fn git(cwd: &Path, args: &[&str]) -> Option<String> {
    let mut child = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(args)
        .env("GIT_TERMINAL_PROMPT", "0")
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()
        .ok()?;
    let mut stdout = child.stdout.take()?;
    let reader = std::thread::spawn(move || {
        let mut text = String::new();
        let _ = stdout.read_to_string(&mut text);
        text
    });
    let deadline = Instant::now() + GIT_TIMEOUT;
    let status = loop {
        match child.try_wait() {
            Ok(Some(status)) => break status,
            Ok(None) if Instant::now() < deadline => std::thread::sleep(Duration::from_millis(10)),
            _ => {
                let _ = child.kill();
                let _ = child.wait();
                return None;
            }
        }
    };
    let text = reader.join().ok()?;
    status.success().then_some(text)
}

/// Le nom de la branche courante ; `None` hors dépôt ou en tête détachée.
pub fn git_branch(cwd: &Path) -> Option<String> {
    // `symbolic-ref` marche aussi sur un dépôt sans commit et échoue en tête détachée.
    let name = git(cwd, &["symbolic-ref", "--short", "-q", "HEAD"])?;
    let name = name.trim();
    (!name.is_empty()).then(|| name.to_string())
}

/// Les messages des 10 derniers commits, du plus récent au plus ancien.
pub fn git_commits(cwd: &Path) -> Option<String> {
    git(cwd, &["log", "-n", "10", "--format=%B"])
}

/// Le contenu de `.kairos.json`.
#[derive(Debug, Deserialize, PartialEq, Eq)]
pub struct ConfigFile {
    pub parent: String,
    #[serde(default)]
    pub label: Option<String>,
    #[serde(default)]
    pub subtask: Option<String>,
}

/// Cherche `.kairos.json` en remontant depuis `cwd` jusqu'à la racine du dépôt (le dossier qui
/// contient `.git`). Invalide ou sans `parent` numérique : ignoré, avec une ligne sur la sortie
/// d'erreur.
pub fn kairos_json(cwd: &Path) -> Option<ConfigFile> {
    for dir in cwd.ancestors().take(32) {
        let file = dir.join(".kairos.json");
        if file.is_file() {
            return read_config(&file);
        }
        if dir.join(".git").exists() {
            return None;
        }
    }
    None
}

fn read_config(file: &PathBuf) -> Option<ConfigFile> {
    let parsed = std::fs::read_to_string(file)
        .map_err(|e| e.to_string())
        .and_then(|text| serde_json::from_str::<ConfigFile>(&text).map_err(|e| e.to_string()));
    match parsed {
        Ok(config)
            if !config.parent.is_empty() && config.parent.bytes().all(|b| b.is_ascii_digit()) =>
        {
            Some(config)
        }
        Ok(_) => {
            eprintln!(
                "kairos hook : {} : « parent » doit être un ID Monday (chiffres)",
                file.display()
            );
            None
        }
        Err(why) => {
            eprintln!("kairos hook : {} ignoré : {why}", file.display());
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn kairos_json_is_found_from_a_subfolder_and_stops_at_the_repo_root() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("repo");
        let sub = root.join("a/b");
        std::fs::create_dir_all(&sub).unwrap();
        std::fs::create_dir(root.join(".git")).unwrap();
        // Un fichier AU-DESSUS de la racine du dépôt ne doit pas être lu.
        std::fs::write(dir.path().join(".kairos.json"), r#"{"parent":"1"}"#).unwrap();
        assert_eq!(kairos_json(&sub), None);
        std::fs::write(
            root.join(".kairos.json"),
            r#"{"parent":"12406850311","label":"Bundle"}"#,
        )
        .unwrap();
        let found = kairos_json(&sub).unwrap();
        assert_eq!(found.parent, "12406850311");
        assert_eq!(found.label.as_deref(), Some("Bundle"));
    }

    #[test]
    fn invalid_configs_are_ignored() {
        let dir = tempfile::tempdir().unwrap();
        for bad in [
            "{ pas du json",
            "{}",
            r#"{"parent":""}"#,
            r#"{"parent":"abc"}"#,
            r#"{"parent":12}"#,
        ] {
            std::fs::write(dir.path().join(".kairos.json"), bad).unwrap();
            assert_eq!(kairos_json(dir.path()), None, "{bad}");
        }
    }

    #[test]
    fn the_branch_is_read_without_commits_and_not_when_detached() {
        let dir = tempfile::tempdir().unwrap();
        let run = |args: &[&str]| {
            let out = Command::new("git")
                .arg("-C")
                .arg(dir.path())
                .args([
                    "-c",
                    "user.name=t",
                    "-c",
                    "user.email=t@t",
                    "-c",
                    "commit.gpgsign=false",
                ])
                .args(args)
                .output()
                .unwrap();
            assert!(out.status.success(), "{args:?}");
        };
        run(&["init", "-q", "-b", "feature/12406850311-x"]);
        assert_eq!(
            git_branch(dir.path()).as_deref(),
            Some("feature/12406850311-x")
        );
        run(&["commit", "-q", "--allow-empty", "-m", "un"]);
        run(&["checkout", "-q", "--detach"]);
        assert_eq!(git_branch(dir.path()), None);
        assert!(git_commits(dir.path()).unwrap().contains("un"));
    }

    #[test]
    fn git_outside_a_repo_or_in_a_missing_folder_is_no_source() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(git_branch(dir.path()), None);
        assert_eq!(git_commits(dir.path()), None);
        assert_eq!(git_branch(&dir.path().join("absent")), None);
    }
}
