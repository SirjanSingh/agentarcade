//! Hook entries for the coding agents other than Claude Code.
//!
//! Claude Code can post to us directly (`hooks.rs`). Codex, Cursor and
//! Antigravity can only run a command, so each hook here is a `curl.exe` that
//! forwards the hook's stdin JSON to our endpoint and names its source in the
//! query string. `curl.exe` ships with Windows 10 1803 and later, so a friend
//! needs nothing installed beyond the app.
//!
//! Same rules as `hooks.rs`: we only add or remove entries that are ours, never
//! touch anything else, and uninstall is an exact inverse of install. An entry
//! is ours if its command posts to `http://127.0.0.1:<port>/event?src=` — on
//! any port, so a port change never leaves a dead hook behind.
//!
//! The only thing interpolated into a command is a `u16` port. Nothing from a
//! payload, a file or the network ever reaches a command string.

use serde_json::{json, Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Agent {
    Codex,
    Cursor,
    Antigravity,
}

impl Agent {
    pub const ALL: [Agent; 3] = [Agent::Codex, Agent::Cursor, Agent::Antigravity];

    /// The `src=` tag, also the id the settings window uses.
    pub fn id(self) -> &'static str {
        match self {
            Agent::Codex => "codex",
            Agent::Cursor => "cursor",
            Agent::Antigravity => "antigravity",
        }
    }

    pub fn from_id(id: &str) -> Option<Agent> {
        Agent::ALL.into_iter().find(|a| a.id() == id)
    }

    pub fn name(self) -> &'static str {
        match self {
            Agent::Codex => "Codex",
            Agent::Cursor => "Cursor",
            Agent::Antigravity => "Antigravity",
        }
    }

    /// The tool's home folder under the user's home. Its existence is how we
    /// tell the tool is installed.
    pub fn home_dir(self) -> &'static [&'static str] {
        match self {
            Agent::Codex => &[".codex"],
            Agent::Cursor => &[".cursor"],
            Agent::Antigravity => &[".gemini"],
        }
    }

    /// The user-level hooks file, relative to the user's home.
    pub fn config_file(self) -> &'static [&'static str] {
        match self {
            Agent::Codex => &[".codex", "hooks.json"],
            Agent::Cursor => &[".cursor", "hooks.json"],
            Agent::Antigravity => &[".gemini", "config", "hooks.json"],
        }
    }

    /// Anything the user must do in the tool itself before hooks run.
    pub fn note(self) -> Option<&'static str> {
        match self {
            Agent::Codex => Some("Codex asks you to approve new hooks once: run /hooks in Codex and trust them."),
            Agent::Cursor => Some("Restart Cursor after connecting."),
            Agent::Antigravity => Some("Restart Antigravity after connecting. Not yet confirmed working; check Open log after a prompt."),
        }
    }

    /// Hook `(event, ev tag, keep stdout)` triples.
    ///
    /// stdout is kept only where the tool reads our reply: Cursor's prompt
    /// hook needs `{"continue":true}`, Antigravity expects JSON from both.
    /// Codex adds a prompt hook's stdout to the model's context, so there it
    /// is always thrown away.
    fn events(self) -> &'static [(&'static str, &'static str, bool)] {
        match self {
            Agent::Codex => &[("UserPromptSubmit", "UserPromptSubmit", false), ("Stop", "Stop", false)],
            Agent::Cursor => &[
                ("beforeSubmitPrompt", "UserPromptSubmit", true),
                ("preCompact", "PreCompact", false),
                ("stop", "Stop", false),
            ],
            Agent::Antigravity => &[("PreInvocation", "PreInvocation", true), ("Stop", "Stop", true)],
        }
    }
}

/// Antigravity keys its hooks by a name we choose.
const ANTIGRAVITY_KEY: &str = "agentarcade";

/// The hand-installed Node bridge from before this module existed, under each
/// name the README ever told people to copy it to. It needs Node on the
/// machine; installing ours replaces it. Our own commands never run a `.js`.
const LEGACY_MARKERS: [&str; 3] = ["claudegame.js", "cursor-hook.js", "agentarcade.js"];

fn url(port: u16, agent: Agent, ev: &str) -> String {
    format!("http://127.0.0.1:{port}/event?src={}&ev={ev}", agent.id())
}

/// The command a hook runs. `-m 2` caps the wait if the app is not running,
/// and `curl` exits non-zero then — which every tool here treats as a
/// non-blocking hook failure, not a refused prompt.
///
/// `"@-"` is quoted on purpose: Codex runs hooks through PowerShell on
/// Windows, where a bare `@-` is a parse error (splatting), so the hook fails
/// before curl ever starts. Quoted, it works in PowerShell, cmd.exe and sh.
pub fn command(port: u16, agent: Agent, ev: &str, keep_stdout: bool, windows: bool) -> String {
    let sink = match (keep_stdout, windows) {
        (true, _) => "",
        (false, true) => " -o NUL",
        (false, false) => " -o /dev/null",
    };
    let curl = if windows { "curl.exe" } else { "curl" };
    format!(
        "{curl} -s -m 2{sink} -X POST -H \"Content-Type: application/json\" --data-binary \"@-\" \"{}\"",
        url(port, agent, ev)
    )
}

/// Is this command one of ours, on any port?
fn is_our_command(cmd: &str) -> bool {
    cmd.contains("http://127.0.0.1:") && cmd.contains("/event?src=")
}

fn is_legacy(cmd: &str) -> bool {
    LEGACY_MARKERS.iter().any(|m| cmd.contains(m))
}

/// A handler object is ours if any of its command fields is.
fn handler_is_ours(h: &Value) -> bool {
    ["command", "commandWindows", "command_windows"]
        .iter()
        .filter_map(|k| h.get(k).and_then(Value::as_str))
        .any(|c| is_our_command(c) || is_legacy(c))
}

fn handler(agent: Agent, port: u16, ev: &str, keep_stdout: bool) -> Value {
    match agent {
        // Codex runs `command` on macOS/Linux and `commandWindows` on Windows.
        // Not async: `codex exec` exits before an async Stop hook runs, so the
        // bout would never end. curl answers in milliseconds anyway.
        Agent::Codex => json!({
            "type": "command",
            "command": command(port, agent, ev, keep_stdout, false),
            "commandWindows": command(port, agent, ev, keep_stdout, true),
            "timeout": 5
        }),
        Agent::Cursor => json!({ "command": command(port, agent, ev, keep_stdout, cfg!(windows)) }),
        Agent::Antigravity => json!({
            "type": "command",
            "command": command(port, agent, ev, keep_stdout, cfg!(windows)),
            "timeout": 5
        }),
    }
}

fn ensure_object(v: &mut Value) -> &mut Map<String, Value> {
    if !v.is_object() {
        *v = Value::Object(Map::new());
    }
    v.as_object_mut().expect("just ensured object")
}

fn ensure_array<'a>(map: &'a mut Map<String, Value>, key: &str) -> &'a mut Vec<Value> {
    let v = map.entry(key.to_string()).or_insert_with(|| Value::Array(Vec::new()));
    if !v.is_array() {
        *v = Value::Array(Vec::new());
    }
    v.as_array_mut().expect("just ensured array")
}

/// Add our hooks. Idempotent, and it first removes any of ours on an old port
/// (and the legacy Node bridge), so the result always has exactly one set.
pub fn install(config: &mut Value, agent: Agent, port: u16) {
    uninstall(config, agent);
    let root = ensure_object(config);
    match agent {
        Agent::Codex => {
            let hooks = ensure_object(root.entry("hooks").or_insert(Value::Null));
            for (event, ev, keep) in agent.events() {
                ensure_array(hooks, event).push(json!({ "hooks": [handler(agent, port, ev, *keep)] }));
            }
        }
        Agent::Cursor => {
            root.entry("version").or_insert(json!(1));
            let hooks = ensure_object(root.entry("hooks").or_insert(Value::Null));
            for (event, ev, keep) in agent.events() {
                ensure_array(hooks, event).push(handler(agent, port, ev, *keep));
            }
        }
        Agent::Antigravity => {
            let mut ours = Map::new();
            for (event, ev, keep) in agent.events() {
                ours.insert(event.to_string(), json!([handler(agent, port, ev, *keep)]));
            }
            root.insert(ANTIGRAVITY_KEY.to_string(), Value::Object(ours));
        }
    }
}

/// Remove every hook of ours, on any port, plus the legacy bridge, and any
/// container that removing them left empty.
pub fn uninstall(config: &mut Value, agent: Agent) {
    let Some(root) = config.as_object_mut() else {
        return;
    };
    match agent {
        Agent::Antigravity => {
            root.remove(ANTIGRAVITY_KEY);
        }
        Agent::Codex | Agent::Cursor => {
            let Some(hooks) = root.get_mut("hooks").and_then(Value::as_object_mut) else {
                return;
            };
            let events: Vec<String> = hooks.keys().cloned().collect();
            for event in events {
                let Some(list) = hooks.get_mut(&event).and_then(Value::as_array_mut) else {
                    continue;
                };
                let before = list.len();
                if agent == Agent::Codex {
                    // Groups of handlers: drop our handlers, then only the
                    // groups we emptied.
                    for g in list.iter_mut() {
                        if let Some(hs) = g.get_mut("hooks").and_then(Value::as_array_mut) {
                            hs.retain(|h| !handler_is_ours(h));
                        }
                    }
                    list.retain(|g| {
                        g.get("hooks").and_then(Value::as_array).is_none_or(|hs| !hs.is_empty())
                    });
                } else {
                    list.retain(|h| !handler_is_ours(h));
                }
                if list.is_empty() && before > 0 {
                    hooks.remove(&event);
                }
            }
            if hooks.is_empty() {
                root.remove("hooks");
                // Cursor's version key is only there because hooks were.
                if agent == Agent::Cursor && root.len() == 1 && root.contains_key("version") {
                    root.remove("version");
                }
            }
        }
    }
}

/// True if our hook is present for every event this agent needs, on `port`.
pub fn is_installed(config: &Value, agent: Agent, port: u16) -> bool {
    let has = |list: Option<&Value>, ev: &str| -> bool {
        let want = url(port, agent, ev);
        let matches = |h: &Value| {
            ["command", "commandWindows"]
                .iter()
                .filter_map(|k| h.get(k).and_then(Value::as_str))
                .any(|c| c.contains(&want))
        };
        list.and_then(Value::as_array).is_some_and(|items| {
            items.iter().any(|item| {
                matches(item)
                    || item
                        .get("hooks")
                        .and_then(Value::as_array)
                        .is_some_and(|hs| hs.iter().any(matches))
            })
        })
    };
    let events = agent.events();
    match agent {
        Agent::Antigravity => {
            let ours = config.get(ANTIGRAVITY_KEY);
            events.iter().all(|(event, ev, _)| has(ours.and_then(|o| o.get(event)), ev))
        }
        _ => {
            let hooks = config.get("hooks");
            events.iter().all(|(event, ev, _)| has(hooks.and_then(|h| h.get(event)), ev))
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const PORT: u16 = 47913;

    #[test]
    fn install_is_idempotent_and_detected() {
        for agent in Agent::ALL {
            let mut cfg = json!({});
            install(&mut cfg, agent, PORT);
            let once = cfg.clone();
            install(&mut cfg, agent, PORT);
            assert_eq!(cfg, once, "{agent:?} doubled its hooks");
            assert!(is_installed(&cfg, agent, PORT), "{agent:?}");
            assert!(!is_installed(&cfg, agent, 1234), "{agent:?} on the wrong port");
        }
    }

    #[test]
    fn uninstall_restores_an_empty_file_exactly() {
        for agent in Agent::ALL {
            let mut cfg = json!({});
            install(&mut cfg, agent, PORT);
            uninstall(&mut cfg, agent);
            assert_eq!(cfg, json!({}), "{agent:?}");
        }
    }

    #[test]
    fn codex_keeps_the_users_own_hooks() {
        let mut cfg = json!({ "hooks": { "Stop": [
            { "hooks": [{ "type": "command", "command": "notify-send done" }] }
        ]}});
        let original = cfg.clone();
        install(&mut cfg, Agent::Codex, PORT);
        assert_eq!(cfg["hooks"]["Stop"].as_array().unwrap().len(), 2);
        uninstall(&mut cfg, Agent::Codex);
        assert_eq!(cfg, original);
    }

    #[test]
    fn cursor_replaces_the_legacy_node_bridge_and_keeps_others() {
        let mut cfg = json!({ "version": 1, "hooks": {
            "beforeSubmitPrompt": [
                { "command": "node \"C:\\Users\\x\\.cursor\\hooks\\claudegame.js\" UserPromptSubmit" },
                { "command": "my-own-hook.exe" }
            ],
            "stop": [{ "command": "node claudegame.js Stop" }]
        }});
        install(&mut cfg, Agent::Cursor, PORT);
        let before = cfg["hooks"]["beforeSubmitPrompt"].as_array().unwrap();
        assert_eq!(before.len(), 2, "legacy gone, user's kept, ours added");
        assert!(before.iter().any(|h| h["command"] == "my-own-hook.exe"));
        assert!(!cfg.to_string().contains("claudegame.js"));

        uninstall(&mut cfg, Agent::Cursor);
        assert_eq!(
            cfg,
            json!({ "version": 1, "hooks": { "beforeSubmitPrompt": [{ "command": "my-own-hook.exe" }] } })
        );
    }

    #[test]
    fn antigravity_leaves_other_named_hooks_alone() {
        let mut cfg = json!({ "my-linter-hook": { "PostToolUse": [] } });
        install(&mut cfg, Agent::Antigravity, PORT);
        assert!(cfg.get("my-linter-hook").is_some());
        uninstall(&mut cfg, Agent::Antigravity);
        assert_eq!(cfg, json!({ "my-linter-hook": { "PostToolUse": [] } }));
    }

    #[test]
    fn a_port_change_replaces_rather_than_adds() {
        for agent in Agent::ALL {
            let mut cfg = json!({});
            install(&mut cfg, agent, 40000);
            install(&mut cfg, agent, PORT);
            assert!(is_installed(&cfg, agent, PORT));
            assert!(!cfg.to_string().contains(":40000/"), "{agent:?} kept the old port");
        }
    }

    #[test]
    fn codex_never_lets_prompt_hook_output_reach_the_model() {
        let mut cfg = json!({});
        install(&mut cfg, Agent::Codex, PORT);
        for event in ["UserPromptSubmit", "Stop"] {
            let h = &cfg["hooks"][event][0]["hooks"][0];
            assert!(h["commandWindows"].as_str().unwrap().contains("-o NUL"));
            assert!(h["command"].as_str().unwrap().contains("-o /dev/null"));
        }
    }

    #[test]
    fn a_command_only_ever_interpolates_the_port() {
        let c = command(PORT, Agent::Cursor, "Stop", false, true);
        assert_eq!(
            c,
            "curl.exe -s -m 2 -o NUL -X POST -H \"Content-Type: application/json\" \
             --data-binary \"@-\" \"http://127.0.0.1:47913/event?src=cursor&ev=Stop\""
        );
    }

    #[test]
    fn a_non_object_file_is_repaired_not_panicked_on() {
        for agent in Agent::ALL {
            let mut cfg = json!([1, 2]);
            install(&mut cfg, agent, PORT);
            assert!(is_installed(&cfg, agent, PORT));
        }
    }
}
