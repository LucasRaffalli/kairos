//! Ce que le hook rend à Claude Code : un JSON sur la sortie standard, ou rien.

use serde_json::{json, Value};

/// Claude Code plafonne ces textes à 10 000 caractères.
const MAX_CHARS: usize = 10_000;

fn cap(text: String) -> String {
    if text.chars().count() <= MAX_CHARS {
        text
    } else {
        text.chars().take(MAX_CHARS).collect()
    }
}

/// Le JSON d'un hook : `additional_context` va dans le contexte de Claude, `system_message` est
/// montré à l'utilisateur. Rien à dire : `None` (on n'écrit rien du tout).
pub fn render(
    event_name: &str,
    additional_context: Option<String>,
    system_message: Option<String>,
) -> Option<String> {
    let additional_context = additional_context.filter(|t| !t.trim().is_empty());
    let system_message = system_message.filter(|t| !t.trim().is_empty());
    if additional_context.is_none() && system_message.is_none() {
        return None;
    }
    let mut out = serde_json::Map::new();
    if let Some(context) = additional_context {
        out.insert(
            "hookSpecificOutput".into(),
            json!({ "hookEventName": event_name, "additionalContext": cap(context) }),
        );
    }
    if let Some(message) = system_message {
        out.insert("systemMessage".into(), Value::String(cap(message)));
    }
    Some(Value::Object(out).to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(s: Option<String>) -> Value {
        serde_json::from_str(&s.unwrap()).unwrap()
    }

    #[test]
    fn both_fields() {
        let v = parse(render(
            "UserPromptSubmit",
            Some("pour Claude".into()),
            Some("pour l'utilisateur".into()),
        ));
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "UserPromptSubmit");
        assert_eq!(v["hookSpecificOutput"]["additionalContext"], "pour Claude");
        assert_eq!(v["systemMessage"], "pour l'utilisateur");
    }

    #[test]
    fn one_field_only_omits_the_other() {
        let v = parse(render("SessionStart", Some("ctx".into()), None));
        assert!(v.get("systemMessage").is_none());
        assert_eq!(v["hookSpecificOutput"]["hookEventName"], "SessionStart");
        let v = parse(render("SessionStart", None, Some("msg".into())));
        assert!(v.get("hookSpecificOutput").is_none());
    }

    #[test]
    fn nothing_to_say_writes_nothing() {
        assert_eq!(render("SessionStart", None, None), None);
        assert_eq!(
            render("SessionStart", Some("  ".into()), Some(String::new())),
            None
        );
    }

    #[test]
    fn long_texts_are_capped() {
        let v = parse(render("SessionStart", Some("é".repeat(20_000)), None));
        let text = v["hookSpecificOutput"]["additionalContext"]
            .as_str()
            .unwrap();
        assert_eq!(text.chars().count(), MAX_CHARS);
    }
}
