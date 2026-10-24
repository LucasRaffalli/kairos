//! Ce que Claude Code envoie à un hook, sur l'entrée standard.

use std::io::Read;
use std::path::PathBuf;

use serde::Deserialize;

/// On ne lit pas plus de 1 Mio : un message énorme ne doit ni bloquer ni gonfler la mémoire.
pub const MAX_INPUT_BYTES: u64 = 1024 * 1024;

#[derive(Debug, Default, Deserialize)]
pub struct HookInput {
    #[serde(default, deserialize_with = "lenient_string")]
    pub session_id: String,
    #[serde(default)]
    pub cwd: Option<PathBuf>,
    #[serde(default, deserialize_with = "lenient_string")]
    pub prompt: String,
}

/// Une chaîne, ou rien : un champ d'un autre type (nombre, objet) vaut la chaîne vide.
fn lenient_string<'de, D: serde::Deserializer<'de>>(d: D) -> Result<String, D::Error> {
    Ok(match serde_json::Value::deserialize(d)? {
        serde_json::Value::String(s) => s,
        _ => String::new(),
    })
}

/// Lit l'entrée d'un hook. Du JSON invalide, tronqué ou vide donne une entrée vide, jamais une erreur.
pub fn read(reader: impl Read) -> HookInput {
    let mut bytes = Vec::new();
    if reader
        .take(MAX_INPUT_BYTES)
        .read_to_end(&mut bytes)
        .is_err()
    {
        return HookInput::default();
    }
    serde_json::from_slice(&bytes).unwrap_or_else(|_| lenient_prompt(&bytes))
}

/// Une entrée tronquée à 1 Mio n'est plus du JSON valide : on en retire le message tel quel
/// (jusqu'à la coupure) pour y chercher quand même un ID, sans rien interpréter d'autre.
fn lenient_prompt(bytes: &[u8]) -> HookInput {
    let text = String::from_utf8_lossy(bytes);
    let Some(at) = text.find("\"prompt\"") else {
        return HookInput::default();
    };
    let rest = &text[at + "\"prompt\"".len()..];
    let Some(open) = rest.find('"') else {
        return HookInput::default();
    };
    let body = &rest[open + 1..];
    let mut prompt = String::new();
    let mut chars = body.chars();
    while let Some(c) = chars.next() {
        match c {
            '"' => break,
            '\\' => match chars.next() {
                Some('n') => prompt.push('\n'),
                Some('t') => prompt.push('\t'),
                Some(other) => prompt.push(other),
                None => break,
            },
            other => prompt.push(other),
        }
    }
    HookInput {
        prompt,
        ..HookInput::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_fields_claude_code_sends_and_ignores_the_rest() {
        let input = read(
            br#"{"session_id":"abc","cwd":"/tmp/x","hook_event_name":"UserPromptSubmit","prompt":"ticket 1","permission_mode":"default","extra":{"a":1}}"#.as_slice(),
        );
        assert_eq!(input.session_id, "abc");
        assert_eq!(input.cwd, Some(PathBuf::from("/tmp/x")));
        assert_eq!(input.prompt, "ticket 1");
    }

    #[test]
    fn invalid_empty_or_mistyped_input_is_an_empty_input() {
        for bad in [
            &b""[..],
            b"pas du json",
            b"{",
            b"[1,2]",
            b"null",
            b"{\"prompt\": 42}",
        ] {
            let input = read(bad);
            assert_eq!(input.prompt, "", "{bad:?}");
            assert_eq!(input.session_id, "");
        }
    }

    #[test]
    fn never_reads_more_than_a_mebibyte() {
        let mut big = br#"{"prompt":""#.to_vec();
        big.extend(vec![b'x'; 3 * 1024 * 1024]);
        big.extend(br#" ticket 12406850311"}"#);
        let input = read(big.as_slice());
        assert!(input.prompt.len() <= MAX_INPUT_BYTES as usize);
        assert!(
            !input.prompt.contains("12406850311"),
            "la fin n'est pas lue"
        );
    }
}
