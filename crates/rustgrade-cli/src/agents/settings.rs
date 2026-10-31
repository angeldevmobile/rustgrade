//! The rustgrade hook in Claude Code's `settings.json`: one entry that `install`
//! adds or updates and `uninstall` removes, never touching the user's other
//! settings.
//!
//! evals/agents measured it working in runs where the agent never loaded the
//! skill, so it reaches agents that the skill and the instructions section miss.

use anyhow::Context;
use serde_json::{Map, Value, json};

/// Silent where `rustgrade hook` can't run (a teammate without the CLI, an older
/// CLI): exit 2 would show the agent an error after every edit. Keep in sync with
/// `hooks/hooks.json`, the same hook for the plugin.
pub const COMMAND: &str = "rustgrade hook 2>/dev/null || true";
const EVENT: &str = "PostToolUse";
const MATCHER: &str = "Edit|Write";

fn is_ours(hook: &Value) -> bool {
    hook["command"]
        .as_str()
        .is_some_and(|c| c.starts_with("rustgrade hook"))
}

/// The entry `install` writes, as `extract` returns it.
pub fn entry() -> String {
    let hook = json!({ "type": "command", "command": COMMAND });
    json!({ "matcher": MATCHER, "hooks": [hook] }).to_string()
}

/// The entry holding the rustgrade hook, if `text` has one.
pub fn extract(text: &str) -> Option<String> {
    let settings: Value = serde_json::from_str(text).ok()?;
    settings["hooks"][EVENT]
        .as_array()?
        .iter()
        .find(|e| e["hooks"].as_array().is_some_and(|h| h.iter().any(is_ours)))
        .map(Value::to_string)
}

fn parse(text: &str) -> anyhow::Result<Map<String, Value>> {
    match serde_json::from_str(text).context("not valid JSON")? {
        Value::Object(settings) => Ok(settings),
        _ => anyhow::bail!("not a JSON object"),
    }
}

/// `existing` (or a new file) with the rustgrade entry in place of the old one,
/// or added after the user's. Fails, changing nothing, if the settings don't have
/// the expected shape.
pub fn upsert(existing: Option<&str>) -> anyhow::Result<String> {
    let mut settings = match existing.filter(|t| !t.trim().is_empty()) {
        Some(text) => parse(text)?,
        None => Map::new(),
    };
    let entry: Value = serde_json::from_str(&entry())?;
    let entries = settings
        .entry("hooks")
        .or_insert_with(|| json!({}))
        .as_object_mut()
        .context("`hooks` is not an object")?
        .entry(EVENT)
        .or_insert_with(|| json!([]))
        .as_array_mut()
        .with_context(|| format!("`hooks.{EVENT}` is not an array"))?;
    let ours = entries
        .iter()
        .position(|e| e["hooks"].as_array().is_some_and(|h| h.iter().any(is_ours)));
    match ours {
        Some(i) => entries[i] = entry,
        None => entries.push(entry),
    }
    Ok(serde_json::to_string_pretty(&settings)? + "\n")
}

/// `text` without the rustgrade hook, and without the entries and keys that held
/// only that. Empty when nothing else is left.
pub fn remove(text: &str) -> anyhow::Result<String> {
    let mut settings = parse(text)?;
    if let Some(hooks) = settings.get_mut("hooks").and_then(Value::as_object_mut) {
        if let Some(entries) = hooks.get_mut(EVENT).and_then(Value::as_array_mut) {
            for entry in entries.iter_mut() {
                if let Some(list) = entry.get_mut("hooks").and_then(Value::as_array_mut) {
                    list.retain(|h| !is_ours(h));
                }
            }
            entries.retain(|e| e["hooks"].as_array().is_none_or(|h| !h.is_empty()));
            if entries.is_empty() {
                hooks.remove(EVENT);
            }
        }
        if hooks.is_empty() {
            settings.remove("hooks");
        }
    }
    if settings.is_empty() {
        return Ok(String::new());
    }
    Ok(serde_json::to_string_pretty(&settings)? + "\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_file_is_just_the_entry_and_removes_to_nothing() {
        let text = upsert(None).expect("upsert");
        assert_eq!(extract(&text), Some(entry()));
        assert_eq!(remove(&text).expect("remove"), "");
        assert_eq!(upsert(Some(" \n")).expect("upsert"), text);
    }

    #[test]
    fn keeps_everything_else_in_order_and_puts_it_back() {
        let original = r#"{
  "permissions": { "allow": ["Bash(cargo *)"] },
  "env": { "A": "1" },
  "hooks": {
    "PostToolUse": [
      { "matcher": "Write", "hooks": [{ "type": "command", "command": "fmt.sh" }] }
    ],
    "Stop": [
      { "hooks": [{ "type": "command", "command": "stop.sh" }] }
    ]
  }
}
"#;
        let with = upsert(Some(original)).expect("upsert");
        assert_eq!(extract(&with), Some(entry()));
        assert!(with.find("permissions") < with.find("\"env\""), "{with}");
        assert!(with.find("fmt.sh") < with.find("rustgrade hook"), "{with}");

        let without = remove(&with).expect("remove");
        assert_eq!(extract(&without), None);
        let (back, was): (Value, Value) = (
            serde_json::from_str(&without).expect("JSON"),
            serde_json::from_str(original).expect("JSON"),
        );
        assert_eq!(back, was);
    }

    #[test]
    fn replaces_an_old_entry_in_place() {
        let old = r#"{"hooks":{"PostToolUse":[
            {"matcher":"Edit","hooks":[{"type":"command","command":"rustgrade hook"}]},
            {"matcher":"Write","hooks":[{"type":"command","command":"fmt.sh"}]}
        ]}}"#;
        assert_ne!(extract(old), Some(entry()));
        let text = upsert(Some(old)).expect("upsert");
        assert_eq!(extract(&text), Some(entry()));
        assert_eq!(text.matches("rustgrade hook").count(), 1);
        assert!(text.find("rustgrade hook") < text.find("fmt.sh"), "{text}");
    }

    #[test]
    fn removes_only_our_hook_from_a_shared_entry() {
        let shared = r#"{"hooks":{"PostToolUse":[{"matcher":"Edit|Write","hooks":[
            {"type":"command","command":"fmt.sh"},
            {"type":"command","command":"rustgrade hook 2>/dev/null || true"}
        ]}]}}"#;
        let text = remove(shared).expect("remove");
        assert!(text.contains("fmt.sh"), "{text}");
        assert!(!text.contains("rustgrade"), "{text}");
    }

    #[test]
    fn refuses_settings_it_does_not_understand() {
        for bad in [
            "{ // comment\n}",
            "[]",
            r#"{"hooks": []}"#,
            r#"{"hooks": {"PostToolUse": {}}}"#,
        ] {
            assert!(upsert(Some(bad)).is_err(), "{bad}");
            assert_eq!(extract(bad), None, "{bad}");
        }
    }
}
