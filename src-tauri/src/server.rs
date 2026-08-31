//! Loopback HTTP endpoint that an agent's `http` hooks post to.
//!
//! Two rules govern this file:
//!   1. Always answer 200 with `{}`. A non-2xx or a timeout is a non-blocking
//!      error for Claude Code, but it still prints a line in the transcript —
//!      the app must never be the reason a prompt looks broken.
//!   2. Never do UI work in the handler. Parse, hand the event to the tracker
//!      task over a channel, return.

use axum::extract::State;
use axum::http::{header::HOST, HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::{routing::get, routing::post, Json, Router};
use serde::Deserialize;
use serde_json::{json, Value};
use tokio::sync::mpsc::Sender;

pub const DEFAULT_PORT: u16 = 47913;

/// What the tracker task needs out of a hook payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HookMessage {
    /// UserPromptSubmit / PreCompact — the agent started working.
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
    // Sessions predate the session_id field in some agent versions, and
    // a missing id would otherwise make every session look like the same one —
    // which is still the right fallback for a single-session user.
    let session_id = parsed.session_id.unwrap_or_else(|| "default".to_string());
    match parsed.hook_event_name.as_str() {
        "UserPromptSubmit" | "PreCompact" => Some(HookMessage::Show { session_id }),
        "Stop" => Some(HookMessage::Stop { session_id }),
        _ => None,
    }
}

/// Only accept requests actually addressed to our loopback endpoint.
///
/// Binding to 127.0.0.1 keeps the LAN out, but it does not stop a page the user
/// is browsing from reaching us via DNS rebinding — that attack resolves an
/// attacker-controlled hostname to 127.0.0.1, so the socket looks local while
/// the request carries the attacker's domain in `Host`. Requiring our own
/// host:port closes it. Worst case here is a window popping up unbidden, not
/// data loss, so a header check is the right size of fix; a token would sit in
/// plaintext in the same settings.json an attacker with local read access
/// already has.
pub fn host_is_ours(headers: &HeaderMap, port: u16) -> bool {
    let Some(host) = headers.get(HOST).and_then(|h| h.to_str().ok()) else {
        // Hooks are HTTP/1.1 and always send Host. Absent means not a browser
        // and not our hook — a bare curl without the header still works because
        // curl sets it.
        return false;
    };
    host == format!("127.0.0.1:{port}") || host == format!("localhost:{port}")
}

#[derive(Clone)]
struct AppCtx {
    tx: Sender<HookMessage>,
    port: u16,
}

async fn handle_event(
    State(ctx): State<AppCtx>,
    headers: HeaderMap,
    Json(payload): Json<Value>,
) -> impl IntoResponse {
    if !host_is_ours(&headers, ctx.port) {
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    if let Some(message) = classify(&payload) {
        // try_send, not send: if the tracker task is wedged we drop the event
        // rather than make the agent wait on us.
        if let Err(err) = ctx.tx.try_send(message) {
            eprintln!("[agentarcade] dropped hook event: {err}");
        }
    }
    (StatusCode::OK, Json(json!({})))
}

async fn health() -> Json<Value> {
    Json(json!({ "ok": true, "app": "agentarcade" }))
}

pub fn router(tx: Sender<HookMessage>, port: u16) -> Router {
    Router::new()
        .route("/event", post(handle_event))
        .route("/health", get(health))
        .with_state(AppCtx { tx, port })
}

/// Bind loopback only — this endpoint takes unauthenticated input and has no
/// business being reachable from the network.
pub async fn serve(port: u16, tx: Sender<HookMessage>) -> std::io::Result<()> {
    let listener = tokio::net::TcpListener::bind(("127.0.0.1", port)).await?;
    eprintln!("[agentarcade] listening on http://127.0.0.1:{port}");
    axum::serve(listener, router(tx, port)).await
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

    fn headers_with_host(value: &str) -> HeaderMap {
        let mut h = HeaderMap::new();
        h.insert(HOST, value.parse().unwrap());
        h
    }

    #[test]
    fn host_check_accepts_our_loopback_endpoint() {
        assert!(host_is_ours(&headers_with_host("127.0.0.1:47913"), 47913));
        assert!(host_is_ours(&headers_with_host("localhost:47913"), 47913));
    }

    #[test]
    fn host_check_rejects_a_rebound_domain() {
        // The DNS-rebinding case: the socket is loopback, the Host is not.
        assert!(!host_is_ours(&headers_with_host("evil.example.com"), 47913));
        assert!(!host_is_ours(
            &headers_with_host("evil.example.com:47913"),
            47913
        ));
    }

    #[test]
    fn host_check_rejects_a_mismatched_port() {
        assert!(!host_is_ours(&headers_with_host("127.0.0.1:9999"), 47913));
    }

    #[test]
    fn host_check_rejects_a_missing_header() {
        assert!(!host_is_ours(&HeaderMap::new(), 47913));
    }
}
