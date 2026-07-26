//! Installing and removing our hook entries in Claude Code's settings.json.
//!
//! This edits a file the user did not write for us, so every operation is
//! surgical: we only ever add or remove entries whose `url` is ours, we never
//! rewrite or reorder anything else, and uninstall is an exact inverse of
//! install. `serde_json`'s `preserve_order` feature keeps the user's key order
//! intact through the round trip.

use serde_json::{json, Map, Value};

/// The three events that drive the app.
pub const EVENTS: [&str; 3] = ["UserPromptSubmit", "PreCompact", "Stop"];

/// One hook entry, as it appears inside an event's group.
fn hook_entry(url: &str) -> Value {
    json!({
        "type": "http",
        "url": url,
        // Short: a hook that cannot reach us should hold up the prompt for as
        // little time as possible.
        "timeout": 2
    })
}

fn group(url: &str) -> Value {
    json!({ "hooks": [hook_entry(url)] })
}

fn is_ours(hook: &Value, url: &str) -> bool {
    hook.get("url").and_then(Value::as_str) == Some(url)
}

/// True if every one of our hooks is already present.
pub fn is_installed(settings: &Value, url: &str) -> bool {
    EVENTS.iter().all(|event| {
        settings
            .get("hooks")
            .and_then(|h| h.get(event))
            .and_then(Value::as_array)
            .is_some_and(|groups| {
                groups.iter().any(|g| {
                    g.get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|hooks| hooks.iter().any(|h| is_ours(h, url)))
                })
            })
    })
}

/// Add our hooks. Idempotent: running it twice leaves one copy of each.
pub fn install(settings: &mut Value, url: &str) {
    if !settings.is_object() {
        *settings = Value::Object(Map::new());
    }
    let root = settings.as_object_mut().expect("just ensured object");
    let hooks = root
        .entry("hooks")
        .or_insert_with(|| Value::Object(Map::new()));
    if !hooks.is_object() {
        *hooks = Value::Object(Map::new());
    }
    let hooks = hooks.as_object_mut().expect("just ensured object");

    for event in EVENTS {
        let groups = hooks
            .entry(event.to_string())
            .or_insert_with(|| Value::Array(Vec::new()));
        if !groups.is_array() {
            *groups = Value::Array(Vec::new());
        }
        let groups = groups.as_array_mut().expect("just ensured array");

        let already = groups.iter().any(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_some_and(|hs| hs.iter().any(|h| is_ours(h, url)))
        });
        if !already {
            groups.push(group(url));
        }
    }
}

/// Remove our hooks and any containers we emptied, leaving everything else be.
pub fn uninstall(settings: &mut Value, url: &str) {
    let Some(root) = settings.as_object_mut() else {
        return;
    };
    let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
        return;
    };

    for event in EVENTS {
        let Some(groups) = hooks.get_mut(event).and_then(Value::as_array_mut) else {
            continue;
        };
        for g in groups.iter_mut() {
            if let Some(list) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                list.retain(|h| !is_ours(h, url));
            }
        }
        // A group we emptied was ours; a group that was already empty was not,
        // so only drop groups whose hook list we actually reduced to nothing.
        groups.retain(|g| {
            g.get("hooks")
                .and_then(Value::as_array)
                .is_none_or(|hs| !hs.is_empty())
        });
        if groups.is_empty() {
            hooks.remove(event);
        }
    }

    if hooks.is_empty() {
        root.remove("hooks");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const URL: &str = "http://127.0.0.1:47913/event";

    #[test]
    fn install_adds_all_three_events() {
        let mut settings = json!({});
        install(&mut settings, URL);
        for event in EVENTS {
            let groups = settings["hooks"][event].as_array().unwrap();
            assert_eq!(groups.len(), 1, "{event}");
            assert_eq!(groups[0]["hooks"][0]["url"], URL);
            assert_eq!(groups[0]["hooks"][0]["type"], "http");
        }
        assert!(is_installed(&settings, URL));
    }

    #[test]
    fn install_is_idempotent() {
        let mut settings = json!({});
        install(&mut settings, URL);
        install(&mut settings, URL);
        install(&mut settings, URL);
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn round_trip_restores_the_original_exactly() {
        let original = json!({
            "model": "opus",
            "permissions": { "allow": ["Bash(npm run *)"] },
            "env": { "FOO": "bar" }
        });
        let mut settings = original.clone();
        install(&mut settings, URL);
        assert_ne!(settings, original);
        uninstall(&mut settings, URL);
        assert_eq!(settings, original, "uninstall must be an exact inverse");
    }

    #[test]
    fn round_trip_preserves_key_order() {
        // Requires serde_json's preserve_order feature; without it this test
        // fails and the user's settings.json would come back reshuffled.
        let original: Value =
            serde_json::from_str(r#"{"zeta":1,"alpha":2,"middle":{"b":1,"a":2}}"#).unwrap();
        let mut settings = original.clone();
        install(&mut settings, URL);
        uninstall(&mut settings, URL);
        assert_eq!(
            serde_json::to_string(&settings).unwrap(),
            serde_json::to_string(&original).unwrap()
        );
    }

    #[test]
    fn uninstall_keeps_unrelated_hooks_on_the_same_event() {
        let original = json!({
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "notify-send done" }] }]
            }
        });
        let mut settings = original.clone();
        install(&mut settings, URL);
        assert_eq!(settings["hooks"]["Stop"].as_array().unwrap().len(), 2);
        uninstall(&mut settings, URL);
        assert_eq!(settings, original, "someone else's Stop hook was collateral damage");
    }

    #[test]
    fn uninstall_keeps_hooks_pointing_at_a_different_port() {
        let other = "http://127.0.0.1:9999/event";
        let mut settings = json!({});
        install(&mut settings, other);
        install(&mut settings, URL);
        uninstall(&mut settings, URL);
        assert!(is_installed(&settings, other));
        assert!(!is_installed(&settings, URL));
    }

    #[test]
    fn uninstall_on_untouched_settings_changes_nothing() {
        let original = json!({ "model": "opus" });
        let mut settings = original.clone();
        uninstall(&mut settings, URL);
        assert_eq!(settings, original);
    }

    #[test]
    fn is_installed_is_false_when_only_some_events_are_present() {
        let mut settings = json!({});
        install(&mut settings, URL);
        settings["hooks"].as_object_mut().unwrap().remove("PreCompact");
        assert!(!is_installed(&settings, URL));
    }

    #[test]
    fn install_repairs_a_malformed_hooks_value() {
        // A hand-edited settings.json can have anything in it; we must not panic.
        let mut settings = json!({ "hooks": "oops" });
        install(&mut settings, URL);
        assert!(is_installed(&settings, URL));
    }

    #[test]
    fn install_on_a_non_object_root_starts_fresh() {
        let mut settings = json!([1, 2, 3]);
        install(&mut settings, URL);
        assert!(is_installed(&settings, URL));
    }
}
