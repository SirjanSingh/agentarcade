//! Loopback HTTP endpoint that Claude Code's `http` hooks post to.
//!
//! Two rules govern this file:
//!   1. Always answer 200 with `{}`. A non-2xx or a timeout is a non-blocking
//!      error for Claude Code, but it still prints a line in the transcript —
//!      the app must never be the reason a prompt looks broken.
//!   2. Never do UI work in the handler. Parse, hand the event to the tracker
//!      task over a channel, return.

use axum::{extract::State, routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

pub const DEFAULT_PORT: u16 = 47913;

/// What the tracker task needs out of a hook payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookMessage {
    /// UserPromptSubmit / PreCompact — Claude started working.
    Show { session_id: String },
    /// Stop — this session finished.
    Stop { session_id: String },
}

#[derive(Debug, Deserialize)]
struct HookPayload {
    #[serde(default)]
    hook_event_name: String,
    #[serde(default)]
    session_id: Option<String>,
}

/// Map a hook payload to a message. Unknown events are ignored.
pub fn classify(payload: &Value) -> Option<HookMessage> {
    let parsed: HookPayload = serde_json::from_value(payload.clone()).ok()?;
    // Sessions predate the session_id field in some Claude Code versions, and
    // a missing id would otherwise make every session look like the same one —
    // which is still the right fallback for a single-session user.
    let session_id = parsed.session_id.unwrap_or_else(|| "default".to_string());
    match parsed.hook_event_name.as_str() {
        "UserPromptSubmit" | "PreCompact" => Some(HookMessage::Show { session_id }),
        "Stop" => Some(HookMessage::Stop { session_id }),
        _ => None,
    }
}

async fn handle_event(State(tx): State<Sender<HookMessage>>, Json(payload): Json<Value>) -> Json<Value> {
    if let Some(message) = classify(&payload) {
        // try_send, not send: if the tracker task is wedged we drop the event
        // rather than make Claude Code wait on us.
        if let Err(err) = tx.try_send(message) {
            eprintln!("[claudegame] dropped hook event: {err}");
        }
    }
    Json(json!({}))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "app": "claudegame" }))
}

pub fn router(tx: Sender<HookMessage>) -> Router {
    Router::new()
        .route("/event", post(handle_event))
        .route("/health", get(health))
        .with_state(tx)
}

/// Bind loopback only — this endpoint takes unauthenticated input and has no
/// business being reachable from the network.
pub async fn serve(port: u16, tx: Sender<HookMessage>) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("[claudegame] listening on http://127.0.0.1:{port}");
    axum::serve(listener, router(tx)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(event: &str, session: &str) -> Value {
        json!({ "hook_event_name": event, "session_id": session, "cwd": "/tmp" })
    }

    #[test]
    fn prompt_submit_shows() {
        assert_eq!(
            classify(&payload("UserPromptSubmit", "abc")),
            Some(HookMessage::Show { session_id: "abc".into() })
        );
    }

    #[test]
    fn precompact_shows_too() {
        assert_eq!(
            classify(&payload("PreCompact", "abc")),
            Some(HookMessage::Show { session_id: "abc".into() })
        );
    }

    #[test]
    fn stop_stops() {
        assert_eq!(
            classify(&payload("Stop", "abc")),
            Some(HookMessage::Stop { session_id: "abc".into() })
        );
    }

    #[test]
    fn other_events_are_ignored() {
        assert_eq!(classify(&payload("PreToolUse", "abc")), None);
        assert_eq!(classify(&payload("SessionEnd", "abc")), None);
        assert_eq!(classify(&json!({})), None);
    }

    #[test]
    fn a_missing_session_id_falls_back_to_one_session() {
        assert_eq!(
            classify(&json!({ "hook_event_name": "Stop" })),
            Some(HookMessage::Stop { session_id: "default".into() })
        );
    }

    #[test]
    fn junk_payloads_do_not_panic() {
        assert_eq!(classify(&json!("hello")), None);
        assert_eq!(classify(&json!([1, 2, 3])), None);
        assert_eq!(classify(&json!({ "hook_event_name": 42 })), None);
    }
}
