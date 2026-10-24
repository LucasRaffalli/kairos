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

/// Cette question a-t-elle déjà été posée dans cette session ?
pub fn already_asked(session_id: &str, key: &str) -> bool {
    if session_id.is_empty() {
        return false;
    }
    load()["sessions"][session_id]
        .as_array()
        .is_some_and(|asked| asked.iter().any(|f| f == &Value::String(fingerprint(key))))
}

/// Note que cette question a été posée.
pub fn mark_asked(session_id: &str, key: &str) {
    if session_id.is_empty() {
        return;
    }
    let mut state = load();
    let print = Value::String(fingerprint(key));
    {
        let asked = state["sessions"]
            .as_object_mut()
            .expect("load() garantit un objet")
            .entry(session_id.to_string())
            .or_insert_with(|| json!([]));
        if let Some(list) = asked.as_array_mut() {
            if !list.contains(&print) {
                list.push(print);
            }
        }
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
    let tmp = target.with_extension("json.tmp");
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
