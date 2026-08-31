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

/// True for any http hook whose URL looks like one of ours on *some* port.
///
/// The port can move (a conflict, or AGENTARCADE_PORT), and an entry left at
/// the old port is dead weight that adds a 2s timeout to every prompt. Matching
/// on shape rather than on the exact current URL is what lets us find those.
fn looks_like_ours(hook: &Value, path: &str) -> bool {
    hook.get("type").and_then(Value::as_str) == Some("http")
        && hook
            .get("url")
            .and_then(Value::as_str)
            .is_some_and(|u| is_our_url_shape(u, path))
}

fn is_our_url_shape(url: &str, path: &str) -> bool {
    let Some(rest) = url
        .strip_prefix("http://127.0.0.1:")
        .or_else(|| url.strip_prefix("http://localhost:"))
    else {
        return false;
    };
    let Some((port, tail)) = rest.split_once('/') else {
        return false;
    };
    !port.is_empty() && port.chars().all(|c| c.is_ascii_digit()) && format!("/{tail}") == path
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

/// Our hook URLs that are present but point somewhere we are not listening.
///
/// Returned rather than removed: a user may have deliberately kept an entry for
/// a second install on another port, so the decision is theirs to confirm.
pub fn find_orphaned(settings: &Value, current_url: &str) -> Vec<String> {
    let path = current_url
        .rsplit_once('/')
        .map(|(_, tail)| format!("/{tail}"))
        .unwrap_or_else(|| "/event".to_string());

    let mut found: Vec<String> = Vec::new();
    let Some(hooks) = settings.get("hooks").and_then(Value::as_object) else {
        return found;
    };
    for event in EVENTS {
        let Some(groups) = hooks.get(event).and_then(Value::as_array) else {
            continue;
        };
        for g in groups {
            let Some(list) = g.get("hooks").and_then(Value::as_array) else {
                continue;
            };
            for h in list {
                if !looks_like_ours(h, &path) {
                    continue;
                }
                let Some(url) = h.get("url").and_then(Value::as_str) else {
                    continue;
                };
                if url != current_url && !found.iter().any(|u| u == url) {
                    found.push(url.to_string());
                }
            }
        }
    }
    found
}

/// Remove every orphaned entry `find_orphaned` reported. Returns how many
/// distinct URLs were dropped.
pub fn remove_orphaned(settings: &mut Value, current_url: &str) -> usize {
    let orphans = find_orphaned(settings, current_url);
    for url in &orphans {
        uninstall(settings, url);
    }
    orphans.len()
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

    #[test]
    fn find_orphaned_detects_a_different_port() {
        let mut settings = json!({});
        install(&mut settings, "http://127.0.0.1:9999/event");
        install(&mut settings, URL);
        assert_eq!(
            find_orphaned(&settings, URL),
            vec!["http://127.0.0.1:9999/event".to_string()]
        );
    }

    #[test]
    fn find_orphaned_ignores_the_current_url() {
        let mut settings = json!({});
        install(&mut settings, URL);
        assert!(find_orphaned(&settings, URL).is_empty());
    }

    #[test]
    fn find_orphaned_ignores_unrelated_third_party_hooks() {
        let settings = json!({
            "hooks": {
                "Stop": [
                    { "hooks": [{ "type": "command", "command": "notify-send done" }] },
                    { "hooks": [{ "type": "http", "url": "https://example.com/webhook" }] }
                ]
            }
        });
        assert!(find_orphaned(&settings, URL).is_empty());
    }

    #[test]
    fn remove_orphaned_leaves_the_current_port_alone() {
        let mut settings = json!({});
        install(&mut settings, "http://127.0.0.1:9999/event");
        install(&mut settings, URL);
        assert_eq!(remove_orphaned(&mut settings, URL), 1);
        assert!(is_installed(&settings, URL));
        assert!(!is_installed(&settings, "http://127.0.0.1:9999/event"));
    }

    #[test]
    fn remove_orphaned_keeps_someone_elses_hooks() {
        let original = json!({
            "hooks": {
                "Stop": [{ "hooks": [{ "type": "command", "command": "echo hi" }] }]
            }
        });
        let mut settings = original.clone();
        assert_eq!(remove_orphaned(&mut settings, URL), 0);
        assert_eq!(settings, original);
    }

    #[test]
    fn url_shape_only_matches_loopback_event_urls() {
        assert!(is_our_url_shape("http://127.0.0.1:1/event", "/event"));
        assert!(is_our_url_shape("http://localhost:47913/event", "/event"));
        assert!(!is_our_url_shape("http://example.com:47913/event", "/event"));
        assert!(!is_our_url_shape("http://127.0.0.1:47913/other", "/event"));
        assert!(!is_our_url_shape("http://127.0.0.1:abc/event", "/event"));
    }
}
