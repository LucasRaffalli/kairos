//! `kairos mcp` sur de vrais tuyaux : on lui parle en JSON-RPC, une ligne par message.

use std::io::{BufRead, BufReader, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::mpsc::{channel, Receiver, RecvTimeoutError};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

/// 2026-10-23 17:00 UTC (heure locale = UTC dans ces tests).
const NOW: i64 = 1_792_774_800;

struct Session {
    child: Child,
    stdin: Option<ChildStdin>,
    lines: Receiver<String>,
    next_id: i64,
    /// Toutes les lignes reçues, pour vérifier que la sortie standard ne porte que du protocole.
    seen: Vec<String>,
}

fn command(db: &std::path::Path) -> Command {
    let mut c = Command::new(env!("CARGO_BIN_EXE_kairos"));
    c.env("KAIROS_DB", db)
        .env("KAIROS_NOW", NOW.to_string())
        .env("KAIROS_TZ_OFFSET", "0");
    c
}

impl Session {
    /// Lance `kairos mcp`, fait l'`initialize` avec ce nom de client, puis `notifications/initialized`.
    fn open(db: &std::path::Path, client: &str) -> Session {
        let mut child = command(db)
            .arg("mcp")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()
            .expect("le binaire kairos se lance");
        let stdin = child.stdin.take();
        let stdout = child.stdout.take().unwrap();
        let (tx, lines) = channel();
        std::thread::spawn(move || {
            for line in BufReader::new(stdout).lines().map_while(Result::ok) {
                if tx.send(line).is_err() {
                    break;
                }
            }
        });
        let mut s = Session {
            child,
            stdin,
            lines,
            next_id: 0,
            seen: Vec::new(),
        };
        let init = s.request(
            "initialize",
            json!({ "protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": { "name": client, "version": "1.0.0" } }),
        );
        assert!(init.get("result").is_some(), "initialize a échoué : {init}");
        s.notify("notifications/initialized", json!({}));
        s
    }

    fn send_raw(&mut self, text: &str) {
        let stdin = self.stdin.as_mut().expect("l'entrée est ouverte");
        stdin.write_all(text.as_bytes()).unwrap();
        stdin.write_all(b"\n").unwrap();
        stdin.flush().unwrap();
    }

    fn notify(&mut self, method: &str, params: Value) {
        self.send_raw(&json!({ "jsonrpc": "2.0", "method": method, "params": params }).to_string());
    }

    fn read_line(&mut self, timeout: Duration) -> String {
        match self.lines.recv_timeout(timeout) {
            Ok(line) => {
                self.seen.push(line.clone());
                line
            }
            Err(RecvTimeoutError::Timeout) => panic!("aucune réponse du serveur en {timeout:?}"),
            Err(RecvTimeoutError::Disconnected) => {
                panic!("le serveur s'est arrêté (sortie standard fermée)")
            }
        }
    }

    /// Envoie une requête et attend la réponse portant son identifiant.
    fn request_within(&mut self, method: &str, params: Value, timeout: Duration) -> Value {
        self.next_id += 1;
        let id = self.next_id;
        self.send_raw(
            &json!({ "jsonrpc": "2.0", "id": id, "method": method, "params": params }).to_string(),
        );
        let deadline = Instant::now() + timeout;
        loop {
            let left = deadline.saturating_duration_since(Instant::now());
            let line = self.read_line(left.max(Duration::from_millis(1)));
            let value: Value = serde_json::from_str(&line)
                .unwrap_or_else(|e| panic!("ligne non JSON sur la sortie standard ({e}) : {line}"));
            if value["id"] == json!(id) {
                return value;
            }
        }
    }

    fn request(&mut self, method: &str, params: Value) -> Value {
        self.request_within(method, params, Duration::from_secs(10))
    }

    /// Appelle un outil : renvoie `(is_error, JSON contenu dans le texte)`.
    fn tool(&mut self, name: &str, args: Value) -> (bool, Value) {
        let response = self.request("tools/call", json!({ "name": name, "arguments": args }));
        let result = response
            .get("result")
            .unwrap_or_else(|| panic!("attendu un résultat, reçu : {response}"));
        let text = result["content"][0]["text"]
            .as_str()
            .unwrap_or_else(|| panic!("pas de texte : {result}"));
        (
            result["isError"].as_bool().unwrap_or(false),
            serde_json::from_str(text).unwrap_or_else(|e| panic!("texte non JSON ({e}) : {text}")),
        )
    }

    fn close_and_wait(mut self) -> std::process::ExitStatus {
        drop(self.stdin.take());
        let deadline = Instant::now() + Duration::from_secs(10);
        loop {
            if let Some(status) = self.child.try_wait().unwrap() {
                return status;
            }
            assert!(
                Instant::now() < deadline,
                "le serveur ne s'arrête pas quand l'entrée se ferme"
            );
            std::thread::sleep(Duration::from_millis(20));
        }
    }
}

impl Drop for Session {
    fn drop(&mut self) {
        let _ = self.child.kill();
    }
}

fn temp_db() -> (tempfile::TempDir, std::path::PathBuf) {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("kairos.db");
    (dir, path)
}

fn cli(db: &std::path::Path, args: &[&str]) -> std::process::Output {
    command(db).args(args).output().unwrap()
}

fn count(db: &std::path::Path, sql: &str) -> i64 {
    rusqlite::Connection::open(db)
        .unwrap()
        .query_row(sql, [], |r| r.get(0))
        .unwrap()
}

#[test]
fn lists_the_seventeen_tools_with_descriptions_and_schemas() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    let listed = s.request("tools/list", json!({}));
    let tools = listed["result"]["tools"]
        .as_array()
        .unwrap_or_else(|| panic!("{listed}"));
    assert_eq!(tools.len(), 17);
    for t in tools {
        assert!(t["name"].as_str().is_some_and(|n| !n.is_empty()), "{t}");
        assert!(
            t["description"].as_str().is_some_and(|d| !d.is_empty()),
            "{t}"
        );
        assert_eq!(t["inputSchema"]["type"], "object", "{t}");
    }
    let names: Vec<_> = tools.iter().map(|t| t["name"].as_str().unwrap()).collect();
    assert!(
        names.contains(&"delete_entry")
            && !names.contains(&"week")
            && !names.iter().any(|n| n.contains("config"))
    );
}

#[test]
fn a_full_session_start_status_stop_report() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    let (is_error, started) = s.tool("start", json!({ "parent": "12406850311" }));
    assert!(!is_error);
    assert_eq!(started["lot"]["subtask"], "Front");
    let (_, status) = s.tool("status", json!({}));
    assert_eq!(status["state"], "running");
    let (_, stopped) = s.tool("stop", json!({}));
    assert_eq!(stopped["lot"]["parent_monday_id"], "12406850311");
    let (is_error, report) = s.tool("report", json!({}));
    assert!(!is_error);
    assert_eq!(report["status"], "open");
}

#[test]
fn a_business_error_is_is_error_and_the_session_continues() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("start", json!({ "parent": "1" }));
    let (is_error, error) = s.tool("start", json!({ "parent": "2" }));
    assert!(is_error);
    assert_eq!(error["error"]["code"], "already_running");
    assert!(error["error"]["message"]
        .as_str()
        .unwrap()
        .contains("déjà en cours"));
    assert_eq!(s.tool("status", json!({})).1["state"], "running");
}

#[test]
fn ask_mode_then_confirm_over_the_wire() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool(
        "add_entry",
        json!({ "parent": "12406850311", "subtask": "Front", "from": "09:00", "to": "10:00" }),
    );
    let (is_error, asked) = s.tool("delete_entry", json!({ "id": 1 }));
    assert!(!is_error);
    assert_eq!(asked["needs_confirmation"], true);
    assert_eq!(
        asked["would"],
        "supprimer l'entrée #1 (Front sur 12406850311, 9 h 00 → 10 h 00)"
    );
    assert_eq!(count(&db, "SELECT COUNT(*) FROM entries"), 1);
    let (_, done) = s.tool("delete_entry", json!({ "id": 1, "confirm": true }));
    assert_eq!(done["deleted"], 1);
    assert_eq!(count(&db, "SELECT COUNT(*) FROM entries"), 0);
}

#[test]
fn auto_mode_over_the_wire() {
    let (_d, db) = temp_db();
    assert!(cli(&db, &["config", "claude_autonomy", "auto"])
        .status
        .success());
    let mut s = Session::open(&db, "claude-code");
    s.tool(
        "add_entry",
        json!({ "name": "Réunion", "from": "09:00", "to": "10:00" }),
    );
    assert_eq!(s.tool("delete_entry", json!({ "id": 1 })).1["deleted"], 1);
}

#[test]
fn the_client_name_decides_the_source() {
    for (client, expected) in [
        ("claude-code", "claude-code"),
        ("Claude Desktop", "claude-desktop"),
        ("", "claude-desktop"),
    ] {
        let (_d, db) = temp_db();
        let mut s = Session::open(&db, client);
        s.tool("start", json!({ "parent": "1" }));
        let source: String = rusqlite::Connection::open(&db)
            .unwrap()
            .query_row("SELECT source FROM lots", [], |r| r.get(0))
            .unwrap();
        assert_eq!(source, expected, "client {client:?}");
    }
}

#[test]
fn unknown_tool_and_bad_params_are_json_rpc_errors_and_the_server_lives_on() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    let unknown = s.request("tools/call", json!({ "name": "week", "arguments": {} }));
    assert!(unknown.get("error").is_some(), "{unknown}");
    let bad = s.request(
        "tools/call",
        json!({ "name": "delete_entry", "arguments": { "id": 1, "confirm": "true" } }),
    );
    assert!(bad.get("error").is_some(), "{bad}");
    let missing = s.request(
        "tools/call",
        json!({ "name": "delete_entry", "arguments": {} }),
    );
    assert!(missing.get("error").is_some(), "{missing}");
    assert_eq!(s.tool("status", json!({})).1["state"], "idle");
}

#[test]
fn an_invalid_json_line_does_not_kill_the_server() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.send_raw("ceci n'est pas du JSON");
    s.send_raw("{\"jsonrpc\": \"2.0\", \"id\": ");
    assert_eq!(s.tool("status", json!({})).1["state"], "idle");
}

#[test]
fn stdout_contains_only_json_rpc_lines() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("start", json!({ "parent": "1" }));
    s.tool("start", json!({ "parent": "2" }));
    s.request("tools/call", json!({ "name": "nope", "arguments": {} }));
    s.tool("stop", json!({}));
    s.send_raw("pas du JSON");
    s.tool("status", json!({}));
    assert!(!s.seen.is_empty());
    for line in &s.seen {
        let value: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("ligne parasite ({e}) : {line}"));
        assert_eq!(value["jsonrpc"], "2.0", "{line}");
    }
}

#[test]
fn closing_stdin_ends_the_process_with_status_0() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("status", json!({}));
    assert_eq!(s.close_and_wait().code(), Some(0));
}

#[test]
fn a_locked_database_is_db_unavailable_not_a_crash() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("status", json!({}));
    let holder = rusqlite::Connection::open(&db).unwrap();
    holder.execute_batch("BEGIN IMMEDIATE").unwrap();
    let started = Instant::now();
    let response = s.request_within(
        "tools/call",
        json!({ "name": "status", "arguments": {} }),
        Duration::from_secs(20),
    );
    assert!(
        started.elapsed() >= Duration::from_millis(4_500),
        "{:?}",
        started.elapsed()
    );
    assert_eq!(response["result"]["isError"], true);
    let text = response["result"]["content"][0]["text"].as_str().unwrap();
    assert!(text.contains("db_unavailable"), "{text}");
    holder.execute_batch("ROLLBACK").unwrap();
    assert_eq!(
        s.tool("status", json!({})).1["state"],
        "idle",
        "la session est toujours vivante"
    );
}

#[test]
fn a_newer_database_is_db_newer() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("status", json!({}));
    rusqlite::Connection::open(&db)
        .unwrap()
        .pragma_update(None, "user_version", 99)
        .unwrap();
    let (is_error, error) = s.tool("status", json!({}));
    assert!(is_error);
    assert_eq!(error["error"]["code"], "db_newer");
}

#[test]
fn two_servers_on_one_database_only_one_start_wins() {
    let (_d, db) = temp_db();
    let mut a = Session::open(&db, "claude-code");
    let mut b = Session::open(&db, "claude-desktop");
    let (a_err, _) = a.tool("start", json!({ "parent": "1" }));
    let (b_err, b_out) = b.tool("start", json!({ "parent": "2" }));
    assert!(!a_err);
    assert!(b_err);
    assert_eq!(b_out["error"]["code"], "already_running");
    assert_eq!(
        count(&db, "SELECT COUNT(*) FROM entries WHERE end_at IS NULL"),
        1
    );
}

#[test]
fn a_burst_of_calls_is_handled_in_the_order_received() {
    for round in 0..15 {
        let (_d, db) = temp_db();
        let mut s = Session::open(&db, "claude-code");
        let calls = [
            ("start", json!({ "parent": "1" })),
            ("stop", json!({})),
            ("start", json!({ "parent": "2" })),
            ("stop", json!({})),
        ];
        let mut ids = Vec::new();
        for (name, args) in &calls {
            s.next_id += 1;
            ids.push(s.next_id);
            s.send_raw(&json!({ "jsonrpc": "2.0", "id": s.next_id, "method": "tools/call", "params": { "name": name, "arguments": args } }).to_string());
        }
        let mut answers = std::collections::HashMap::new();
        while answers.len() < ids.len() {
            let line = s.read_line(Duration::from_secs(10));
            let value: Value = serde_json::from_str(&line).unwrap();
            answers.insert(value["id"].as_i64().unwrap(), value);
        }
        for id in &ids {
            assert_ne!(
                answers[id]["result"]["isError"], true,
                "tour {round}, appel {id} : {}",
                answers[id]
            );
        }
        assert_eq!(
            s.tool("status", json!({})).1["state"],
            "idle",
            "tour {round}"
        );
    }
}

#[test]
fn an_invalid_task_name_is_a_business_error() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    for name in ["a".repeat(500), "deux\nlignes".to_string()] {
        let (is_error, out) = s.tool("start", json!({ "name": name }));
        assert!(is_error, "{out}");
        assert_eq!(out["error"]["code"], "invalid_name");
    }
    assert_eq!(s.tool("status", json!({})).1["state"], "idle");
}

#[test]
fn delete_of_an_unknown_id_in_ask_mode_is_entry_not_found() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    let (is_error, out) = s.tool("delete_entry", json!({ "id": 42 }));
    assert!(is_error);
    assert_eq!(out["error"]["code"], "entry_not_found");
}

#[test]
fn stdin_closed_mid_session_leaves_the_database_consistent() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool("start", json!({ "parent": "1" }));
    s.send_raw(&json!({ "jsonrpc": "2.0", "id": 99, "method": "tools/call", "params": { "name": "stop", "arguments": {} } }).to_string());
    assert_eq!(s.close_and_wait().code(), Some(0));
    // Le `stop` en vol a été validé en entier ou pas du tout : jamais d'état à moitié écrit.
    let open = count(&db, "SELECT COUNT(*) FROM entries WHERE end_at IS NULL");
    let total = count(&db, "SELECT COUNT(*) FROM entries");
    assert!([(0, 0), (1, 1)].contains(&(open, total)), "{open}/{total}");
    let check: String = rusqlite::Connection::open(&db)
        .unwrap()
        .query_row("PRAGMA integrity_check", [], |r| r.get(0))
        .unwrap();
    assert_eq!(check, "ok");
}

#[test]
fn recent_and_back_over_the_wire() {
    let (_d, db) = temp_db();
    let mut s = Session::open(&db, "claude-code");
    s.tool(
        "start",
        json!({ "parent": "12406850311", "label": "Bundle", "at": "08:00" }),
    );
    // L'horloge est figée : sans heure de départ en arrière, le lot quitté aurait une durée nulle.
    s.tool("switch", json!({ "parent": "12406856533" }));
    let (is_error, recent) = s.tool("recent", json!({}));
    assert!(!is_error);
    assert_eq!(recent[0]["lot"]["parent_monday_id"], "12406856533");
    let (is_error, back) = s.tool("back", json!({}));
    assert!(!is_error, "{back}");
    assert_eq!(back["started"]["lot"]["parent_monday_id"], "12406850311");
    let (_, labelled) = s.tool(
        "set_label",
        json!({ "parent": "12406850311", "label": "Bundle v2" }),
    );
    assert_eq!(labelled["saved"], true);
}
