//! `kairos hook` sur le vrai binaire : le JSON de Claude Code entre par l'entrée standard,
//! un JSON (ou rien) sort sur la sortie standard, et le code de sortie est toujours 0.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// 2026-10-23 17:00 UTC (heure locale = UTC dans ces tests).
const NOW: i64 = 1_792_774_800;
const A: &str = "12406850311";
const B: &str = "12406856533";
const C: &str = "12406850412";

struct Env {
    dir: tempfile::TempDir,
}

struct Hook {
    code: Option<i32>,
    stdout: String,
    stderr: String,
}

impl Hook {
    fn json(&self) -> Value {
        serde_json::from_str(&self.stdout)
            .unwrap_or_else(|e| panic!("sortie non JSON ({e}) : {:?}", self.stdout))
    }

    fn context(&self) -> String {
        self.json()["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn message(&self) -> String {
        self.json()["systemMessage"]
            .as_str()
            .unwrap_or("")
            .to_string()
    }

    fn silent(&self) -> bool {
        self.stdout.trim().is_empty()
    }
}

impl Env {
    fn new() -> Env {
        Env {
            dir: tempfile::tempdir().unwrap(),
        }
    }

    fn db(&self) -> PathBuf {
        self.dir.path().join("kairos.db")
    }

    fn command(&self, now: i64) -> Command {
        let mut c = Command::new(env!("CARGO_BIN_EXE_kairos"));
        c.env("KAIROS_DB", self.db())
            .env("KAIROS_NOW", now.to_string())
            .env("KAIROS_TZ_OFFSET", "0");
        c
    }

    fn cli(&self, now: i64, args: &[&str]) -> Value {
        let out = self.command(now).arg("--json").args(args).output().unwrap();
        serde_json::from_slice(&out.stdout).unwrap_or(Value::Null)
    }

    fn autonomy(&self, value: &str) {
        let out = self
            .command(NOW)
            .args(["config", "claude_autonomy", value])
            .output()
            .unwrap();
        assert!(out.status.success());
    }

    fn status(&self, now: i64) -> Value {
        self.cli(now, &["status"])
    }

    fn hook_raw(&self, now: i64, event: &str, stdin: &[u8]) -> Hook {
        let mut child = self
            .command(now)
            .args(["hook", event])
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .unwrap();
        let mut input = child.stdin.take().unwrap();
        let payload = stdin.to_vec();
        let writer = std::thread::spawn(move || {
            let _ = input.write_all(&payload);
        });
        let out = child.wait_with_output().unwrap();
        let _ = writer.join();
        Hook {
            code: out.status.code(),
            stdout: String::from_utf8_lossy(&out.stdout).to_string(),
            stderr: String::from_utf8_lossy(&out.stderr).to_string(),
        }
    }

    fn prompt_at(&self, now: i64, session: &str, cwd: Option<&Path>, prompt: &str) -> Hook {
        let mut input = json!({
            "session_id": session,
            "hook_event_name": "UserPromptSubmit",
            "prompt": prompt,
        });
        if let Some(cwd) = cwd {
            input["cwd"] = json!(cwd);
        }
        self.hook_raw(now, "user-prompt", input.to_string().as_bytes())
    }

    fn prompt(&self, session: &str, prompt: &str) -> Hook {
        self.prompt_at(NOW, session, None, prompt)
    }

    fn session_start(&self, now: i64) -> Hook {
        let input =
            json!({ "session_id": "s", "hook_event_name": "SessionStart", "source": "startup" });
        self.hook_raw(now, "session-start", input.to_string().as_bytes())
    }
}

fn running_parent(status: &Value) -> Option<String> {
    status["lot"]["parent_monday_id"].as_str().map(String::from)
}

/// Un dépôt Git temporaire sur la branche donnée, avec ces messages de commit (du plus ancien au plus récent).
fn repo(branch: &str, commits: &[&str]) -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    let git = |args: &[&str]| {
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
        assert!(
            out.status.success(),
            "git {args:?} : {}",
            String::from_utf8_lossy(&out.stderr)
        );
    };
    git(&["init", "-q", "-b", branch]);
    for message in commits {
        git(&["commit", "-q", "--allow-empty", "-m", message]);
    }
    dir
}

#[test]
fn auto_starts_a_lot_from_a_sure_id_in_the_message() {
    let e = Env::new();
    e.autonomy("auto");
    let h = e.prompt("s1", &format!("on attaque Bundle, ticket {A}"));
    assert_eq!(h.code, Some(0), "{}", h.stderr);
    assert_eq!(running_parent(&e.status(NOW)).as_deref(), Some(A));
    let message = h.message();
    assert!(
        message.contains("lancé") && message.contains("nouveau parent"),
        "{message}"
    );
    assert!(h.context().contains("set_label"), "{}", h.context());
    assert_eq!(
        h.json()["hookSpecificOutput"]["hookEventName"],
        "UserPromptSubmit"
    );
}

#[test]
fn auto_switches_when_another_id_is_in_the_message_and_says_how_to_go_back() {
    let e = Env::new();
    e.autonomy("auto");
    e.prompt_at(NOW, "s1", None, &format!("feature Bundle ticket {A}"));
    let h = e.prompt_at(
        NOW + 600,
        "s1",
        None,
        &format!("urgence de Manon ticket {B}"),
    );
    assert_eq!(running_parent(&e.status(NOW + 600)).as_deref(), Some(B));
    let message = h.message();
    assert!(message.contains("basculé"), "{message}");
    assert!(
        message.contains(&format!("kairos switch --parent {A}")),
        "{message}"
    );
}

#[test]
fn the_running_lot_cited_again_is_silent() {
    let e = Env::new();
    e.autonomy("auto");
    e.prompt("s1", &format!("ticket {A}"));
    let h = e.prompt_at(NOW + 60, "s1", None, &format!("toujours sur le ticket {A}"));
    assert!(h.silent(), "{}", h.stdout);
}

#[test]
fn ask_mode_never_starts_a_lot_it_only_informs_claude() {
    let e = Env::new();
    let h = e.prompt("s1", &format!("on attaque ticket {A}"));
    assert_eq!(e.status(NOW)["state"], "idle");
    let context = h.context();
    assert!(
        context.contains(A) && context.contains("switch"),
        "{context}"
    );
    assert!(
        h.message().is_empty(),
        "rien n'a changé : pas de message visible"
    );
}

#[test]
fn doubts_are_never_acted_on_and_ask_claude_to_ask() {
    let e = Env::new();
    e.autonomy("auto");
    for (session, prompt, needle) in [
        ("a", format!("entre ticket {A} et ticket {B}"), B),
        (
            "b",
            "mon numéro de commande est 1792774800".to_string(),
            "1792774800",
        ),
    ] {
        let h = e.prompt(session, &prompt);
        assert_eq!(e.status(NOW)["state"], "idle", "{prompt}");
        let context = h.context();
        assert!(context.starts_with("Doute : "), "{context}");
        assert!(
            context.contains("Demande à l'utilisateur") && context.contains(needle),
            "{context}"
        );
    }
}

#[test]
fn the_same_doubt_is_not_repeated_in_a_session_but_is_in_another() {
    let e = Env::new();
    e.autonomy("auto");
    let prompt = format!("entre ticket {A} et ticket {B}");
    assert!(!e.prompt("s1", &prompt).silent());
    assert!(e.prompt("s1", &prompt).silent());
    assert!(!e.prompt("s2", &prompt).silent());
}

#[test]
fn branch_and_commits_with_three_parents() {
    let e = Env::new();
    e.autonomy("auto");
    let dir = repo(
        "feature/bundle",
        &[
            &format!("[Monday #{A}] premier"),
            &format!("fix: {B}"),
            &format!("[Monday #{C}] dernier"),
        ],
    );
    // Rien en cours : une question qui liste les trois candidats.
    let h = e.prompt_at(NOW, "s1", Some(dir.path()), "continue");
    assert_eq!(e.status(NOW)["state"], "idle");
    let context = h.context();
    assert!(
        context.starts_with("Doute : ")
            && context.contains(A)
            && context.contains(B)
            && context.contains(C),
        "{context}"
    );
    // Un des trois en cours : silence.
    e.cli(NOW, &["start", "--parent", B]);
    let h = e.prompt_at(NOW + 60, "s2", Some(dir.path()), "continue");
    assert!(h.silent(), "{}", h.stdout);
}

#[test]
fn a_running_lot_outside_the_branch_parents_is_a_doubt() {
    let e = Env::new();
    e.autonomy("auto");
    let dir = repo(&format!("feature/{A}-bundle"), &[]);
    e.cli(NOW, &["start", "--parent", B]);
    let h = e.prompt_at(NOW + 60, "s1", Some(dir.path()), "continue");
    assert_eq!(running_parent(&e.status(NOW + 60)).as_deref(), Some(B));
    let context = h.context();
    assert!(
        context.starts_with("Doute : ") && context.contains(A) && context.contains(B),
        "{context}"
    );
}

#[test]
fn a_commit_citing_the_running_lot_among_three_ids_is_silent() {
    let e = Env::new();
    let dir = repo("main", &[&format!("[Monday #{A}, #{B}, #{C}] gros commit")]);
    e.cli(NOW, &["start", "--parent", C]);
    assert!(e.prompt_at(NOW + 60, "s1", Some(dir.path()), "go").silent());
}

#[test]
fn a_kairos_json_starts_a_lot_with_its_label_and_subtask() {
    let e = Env::new();
    e.autonomy("auto");
    let dir = repo("main", &[]);
    std::fs::write(
        dir.path().join(".kairos.json"),
        json!({ "parent": A, "label": "Bundle", "subtask": "Back" }).to_string(),
    )
    .unwrap();
    let sub = dir.path().join("src/deep");
    std::fs::create_dir_all(&sub).unwrap();
    e.prompt_at(NOW, "s1", Some(&sub), "go");
    let status = e.status(NOW);
    assert_eq!(status["lot"]["parent_monday_id"], A);
    assert_eq!(status["lot"]["parent_label"], "Bundle");
    assert_eq!(status["lot"]["subtask"], "Back");
}

#[test]
fn an_invalid_kairos_json_is_ignored() {
    let e = Env::new();
    e.autonomy("auto");
    let dir = repo("main", &[]);
    std::fs::write(dir.path().join(".kairos.json"), "{ pas du json").unwrap();
    let h = e.prompt_at(NOW, "s1", Some(dir.path()), "go");
    assert_eq!(h.code, Some(0));
    assert_eq!(e.status(NOW)["state"], "idle");
}

#[test]
fn the_label_hint_comes_once_per_parent_and_session() {
    let e = Env::new();
    e.autonomy("auto");
    let first = e.prompt_at(NOW, "s1", None, &format!("ticket {A}"));
    assert!(first.context().contains("set_label"));
    e.cli(NOW + 60, &["stop"]);
    let second = e.prompt_at(NOW + 120, "s1", None, &format!("ticket {A}"));
    assert_eq!(running_parent(&e.status(NOW + 120)).as_deref(), Some(A));
    assert!(
        !second.context().contains("set_label"),
        "{}",
        second.context()
    );
}

#[test]
fn session_start_gives_the_state_and_the_rules() {
    let e = Env::new();
    let idle = e.session_start(NOW);
    let context = idle.context().to_lowercase();
    assert!(context.contains("aucun lot"), "{context}");
    for rule in [
        "ne jamais arrêter un lot en fin de réponse",
        "ne devine pas",
        "recent",
        "back",
        "set_label",
    ] {
        assert!(context.contains(rule), "règle absente : {rule}");
    }
    assert_eq!(
        idle.json()["hookSpecificOutput"]["hookEventName"],
        "SessionStart"
    );
    e.cli(NOW, &["start", "--parent", A, "--label", "Bundle"]);
    let running = e.session_start(NOW + 600).context();
    assert!(
        running.contains("Bundle") || running.contains(A),
        "{running}"
    );
    assert!(running.contains("0 h 10"), "{running}");
}

#[test]
fn bad_inputs_never_break_the_prompt() {
    let e = Env::new();
    e.autonomy("auto");
    let huge = vec![b'x'; 2 * 1024 * 1024];
    let mut with_id_far = b"{\"prompt\": \"".to_vec();
    with_id_far.extend(vec![b'y'; 2 * 1024 * 1024]);
    with_id_far.extend(format!(" ticket {A}\"}}").into_bytes());
    for (event, input) in [
        ("user-prompt", b"".to_vec()),
        ("user-prompt", b"pas du json".to_vec()),
        ("user-prompt", huge.clone()),
        ("user-prompt", with_id_far),
        ("user-prompt", b"{}".to_vec()),
        ("user-prompt", b"{\"prompt\": 42}".to_vec()),
        ("session-start", b"".to_vec()),
        ("session-start", huge),
    ] {
        let h = e.hook_raw(NOW, event, &input);
        assert_eq!(
            h.code,
            Some(0),
            "{event} {:?}",
            &input[..input.len().min(20)]
        );
        if !h.silent() {
            h.json();
        }
    }
    assert_eq!(
        e.status(NOW)["state"],
        "idle",
        "aucun lot lancé par une entrée invalide"
    );
}

#[test]
fn a_locked_database_makes_the_hook_silent_not_failing() {
    let e = Env::new();
    e.cli(NOW, &["status"]);
    let holder = rusqlite::Connection::open(e.db()).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let h = e.prompt("s1", &format!("ticket {A}"));
    assert!(started.elapsed() < Duration::from_secs(12));
    assert_eq!(h.code, Some(0));
    assert!(h.silent(), "{}", h.stdout);
    holder.execute_batch("ROLLBACK").unwrap();
}

#[test]
fn a_newer_database_makes_the_hook_silent_not_failing() {
    let e = Env::new();
    e.cli(NOW, &["status"]);
    rusqlite::Connection::open(e.db())
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let h = e.prompt("s1", &format!("ticket {A}"));
    assert_eq!(h.code, Some(0));
    assert!(h.silent());
}

#[test]
fn an_unreadable_or_unwritable_state_file_does_not_stop_the_hook() {
    let e = Env::new();
    e.autonomy("auto");
    let state = e.dir.path().join("hook-state.json");
    std::fs::write(&state, "n'importe quoi \u{0}\u{1}").unwrap();
    let prompt = format!("entre ticket {A} et ticket {B}");
    let h = e.prompt("s1", &prompt);
    assert_eq!(h.code, Some(0));
    assert!(h.context().starts_with("Doute : "));
    // Un dossier à la place du fichier : impossible à écrire, on redemande, sans échouer.
    std::fs::remove_file(&state).unwrap();
    std::fs::create_dir(&state).unwrap();
    let again = e.prompt("s1", &prompt);
    assert_eq!(again.code, Some(0));
    assert!(again.context().starts_with("Doute : "));
}

#[test]
fn the_text_of_the_prompt_is_never_written() {
    let e = Env::new();
    e.autonomy("auto");
    let secret = "phrase-secrete-xyzzy-42";
    e.prompt("s1", &format!("{secret} ticket {A} et ticket {B}"));
    e.prompt("s2", &format!("{secret} ticket {A}"));
    for entry in std::fs::read_dir(e.dir.path()).unwrap() {
        let path = entry.unwrap().path();
        if path.is_file() {
            let bytes = std::fs::read(&path).unwrap();
            assert!(
                !bytes.windows(secret.len()).any(|w| w == secret.as_bytes()),
                "le texte du message est dans {path:?}"
            );
        }
    }
}
