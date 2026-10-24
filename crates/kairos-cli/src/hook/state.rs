//! La mémoire des doutes déjà posés, pour ne pas redemander à chaque message.
//!
//! Un petit fichier `hook-state.json` à côté de la base : les 50 dernières sessions, et pour
//! chacune l'empreinte des questions déjà posées. Jamais le texte d'un message. Toute erreur est
//! ignorée : au pire, la question est reposée.

use std::path::PathBuf;

use serde_json::{json, Value};

const MAX_SESSIONS: usize = 50;

fn path() -> PathBuf {
    kairos_core::db::Db::default_path().with_file_name("hook-state.json")
}

/// L'empreinte (FNV-1a, 64 bits) d'une question : stable d'une exécution à l'autre.
fn fingerprint(key: &str) -> String {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for byte in key.bytes() {
        hash ^= u64::from(byte);
        hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
    }
    format!("{hash:016x}")
}

fn load() -> Value {
    std::fs::read_to_string(path())
        .ok()
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
        .filter(|v| v["sessions"].is_object() && v["order"].is_array())
        .unwrap_or_else(|| json!({ "sessions": {}, "order": [] }))
}

/// Ce que la session a déjà entendu : `seen` (des questions posées une fois pour toutes) et
/// `latest` (pour chaque sujet, la dernière question : elle ne se répète que si elle change).
fn session<'a>(state: &'a Value, id: &str) -> &'a Value {
    &state["sessions"][id]
}

/// Cette question a-t-elle déjà été posée dans cette session ?
pub fn already_asked(session_id: &str, key: &str) -> bool {
    !session_id.is_empty()
        && session(&load(), session_id)["seen"]
            .as_array()
            .is_some_and(|seen| seen.contains(&Value::String(fingerprint(key))))
}

/// La dernière question de ce sujet est-elle justement celle-ci ?
pub fn is_latest(session_id: &str, topic: &str, key: &str) -> bool {
    !session_id.is_empty()
        && session(&load(), session_id)["latest"][topic] == Value::String(fingerprint(key))
}

/// Note que cette question a été posée (une fois pour toutes).
pub fn mark_asked(session_id: &str, key: &str) {
    update(session_id, |entry| {
        let seen = entry["seen"].as_array_mut().expect("créé par update");
        let print = Value::String(fingerprint(key));
        if !seen.contains(&print) {
            seen.push(print);
        }
    });
}

/// Note que c'est la dernière question de ce sujet (elle remplace la précédente).
pub fn mark_latest(session_id: &str, topic: &str, key: &str) {
    update(session_id, |entry| {
        entry["latest"][topic] = Value::String(fingerprint(key));
    });
}

/// Met à jour la fiche d'une session, la range en dernier et ne garde que les 50 dernières.
fn update(session_id: &str, change: impl FnOnce(&mut Value)) {
    if session_id.is_empty() {
        return;
    }
    let mut state = load();
    {
        let sessions = state["sessions"]
            .as_object_mut()
            .expect("load() garantit un objet");
        let entry = sessions
            .entry(session_id.to_string())
            .or_insert_with(|| json!({}));
        if !entry["seen"].is_array() {
            *entry = json!({ "seen": [], "latest": {} });
        }
        change(entry);
    }
    let mut order: Vec<Value> = state["order"].as_array().cloned().unwrap_or_default();
    order.retain(|s| s != session_id);
    order.push(Value::String(session_id.to_string()));
    while order.len() > MAX_SESSIONS {
        let old = order.remove(0);
        if let (Some(name), Some(sessions)) = (old.as_str(), state["sessions"].as_object_mut()) {
            sessions.remove(name);
        }
    }
    state["order"] = Value::Array(order);
    // Écriture en deux temps : un hook interrompu ne laisse pas un fichier à moitié écrit.
    let target = path();
    let tmp = target.with_extension(format!("json.tmp{}", std::process::id()));
    if std::fs::write(&tmp, state.to_string()).is_ok() {
        let _ = std::fs::rename(&tmp, &target);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fingerprints_are_stable_and_distinguish_texts() {
        assert_eq!(fingerprint("a"), fingerprint("a"));
        assert_ne!(fingerprint("a"), fingerprint("b"));
        assert_eq!(fingerprint("").len(), 16);
    }
}
