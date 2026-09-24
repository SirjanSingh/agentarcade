//! Loopback HTTP endpoint that coding agents' hooks post to.
//!
//! Claude Code posts here directly (an `http` hook). Codex, Cursor and
//! Antigravity only run commands, so their hooks pipe the hook's stdin JSON
//! through `curl.exe --data-binary @-` and name themselves in the query string:
//! `/event?src=cursor&ev=Stop`. Each tool spells its fields differently, and
//! this file is the one place that knows how.
//!
//! Two rules govern this file:
//!   1. Always answer 200. A non-2xx or a timeout is a non-blocking error for
//!      the agent, but it still shows up in its transcript, and the app must
//!      never be the reason a prompt looks broken. The body is `{}` except
//!      where a tool reads our reply back (see `reply_for`).
//!   2. Never do UI work in the handler. Parse, hand the event to the tracker
//!      task over a channel, return.

use axum::body::Bytes;
use axum::extract::{Query, State};
use axum::http::{header::HOST, header::ORIGIN, HeaderMap, StatusCode};
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

/// Which agent a hook came from and, for tools that do not put the event
/// name in their payload, which event it was. Both come from the query string
/// our installed hook commands carry.
#[derive(Debug, Default, Deserialize)]
pub struct EventQuery {
    #[serde(default)]
    pub src: Option<String>,
    #[serde(default)]
    pub ev: Option<String>,
}

fn str_field<'a>(payload: &'a Value, keys: &[&str]) -> Option<&'a str> {
    keys.iter().find_map(|k| payload.get(k).and_then(Value::as_str))
}

/// Map a hook to a message. Unknown events are ignored.
pub fn classify(payload: &Value, query: &EventQuery) -> Option<HookMessage> {
    let event = query
        .ev
        .as_deref()
        .or_else(|| str_field(payload, &["hook_event_name"]))?;

    // Antigravity has no prompt-submitted event, so its hook fires on every
    // model call. Only the first call of a turn starts the clock; the rest
    // would re-arm a game the user just dismissed with Esc.
    if event == "PreInvocation" {
        if let Some(n) = payload.get("invocationNum").and_then(Value::as_u64) {
            if n != 0 {
                return None;
            }
        }
    }

    // Claude Code and Codex say session_id, Cursor conversation_id,
    // Antigravity conversationId. A missing id falls back to one shared
    // session, which is right for a single-session user.
    let id = str_field(payload, &["session_id", "conversation_id", "conversationId"])
        .unwrap_or("default");
    // Two tools can mint the same id; namespacing keeps one tool's Stop from
    // ending another tool's bout.
    let session_id = match query.src.as_deref() {
        Some(src) => format!("{src}:{id}"),
        None => id.to_string(),
    };

    match event {
        "UserPromptSubmit" | "PreCompact" | "beforeSubmitPrompt" | "preCompact"
        | "PreInvocation" => Some(HookMessage::Show { session_id }),
        "Stop" | "stop" => Some(HookMessage::Stop { session_id }),
        _ => None,
    }
}

/// The body a hook's own tool reads back. Cursor's prompt hook needs
/// explicit permission to continue; Antigravity's Stop hook must say the
/// agent may stop (anything but "continue" does). Everyone else gets `{}`.
pub fn reply_for(query: &EventQuery) -> Value {
    match query.src.as_deref() {
        Some("cursor") => json!({ "continue": true }),
        Some("antigravity") => json!({ "decision": "stop" }),
        _ => json!({}),
    }
}

/// Refuse anything a web browser sent.
///
/// The Host check stops DNS rebinding, but a page the user is browsing can
/// still `fetch("http://127.0.0.1:47913/event")` directly: that request
/// carries our own Host. Browsers always attach `Origin` (and modern ones
/// `Sec-Fetch-*`) to a cross-site POST; curl and the agents' hook runners
/// never do. So their presence alone marks a request as not ours.
pub fn sent_by_a_browser(headers: &HeaderMap) -> bool {
    headers.contains_key(ORIGIN)
        || headers.contains_key("sec-fetch-site")
        || headers.contains_key("sec-fetch-mode")
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
    Query(query): Query<EventQuery>,
    body: Bytes,
) -> impl IntoResponse {
    if !host_is_ours(&headers, ctx.port) || sent_by_a_browser(&headers) {
        return (StatusCode::FORBIDDEN, Json(json!({})));
    }
    // Raw bytes rather than the Json extractor: a hook runner that omits the
    // content type (or a Windows tool that prepends a BOM) must still get a
    // 200, per rule 1. A body that is not JSON at all is just an empty payload,
    // which works for hooks that carry their event in the query string.
    let text = String::from_utf8_lossy(&body);
    let payload: Value =
        serde_json::from_str(text.trim_start_matches('\u{feff}')).unwrap_or(Value::Null);

    let message = classify(&payload, &query);
    alog!(
        "hook src={} -> {:?}",
        query.src.as_deref().unwrap_or("claude"),
        message
    );
    if let Some(message) = message {
        // try_send, not send: if the tracker task is wedged we drop the event
        // rather than make the agent wait on us.
        if let Err(err) = ctx.tx.try_send(message) {
            alog!("dropped hook event: {err}");
        }
    }
    (StatusCode::OK, Json(reply_for(&query)))
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
    alog!("listening on http://127.0.0.1:{port}");
    axum::serve(listener, router(tx, port)).await
}

#[cfg(test)]
mod tests {
    use super::*;

    fn payload(event: &str, session: &str) -> Value {
        json!({ "hook_event_name": event, "session_id": session, "cwd": "/tmp" })
    }

    fn classify(payload: &Value) -> Option<HookMessage> {
        super::classify(payload, &EventQuery::default())
    }

    fn from(src: &str, ev: Option<&str>) -> EventQuery {
        EventQuery {
            src: Some(src.into()),
            ev: ev.map(Into::into),
        }
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

    // --- other agents -----------------------------------------------------

    #[test]
    fn codex_payloads_pass_straight_through_with_a_namespaced_session() {
        let body = json!({ "hook_event_name": "UserPromptSubmit", "session_id": "s1",
                           "turn_id": "t", "prompt": "hi" });
        assert_eq!(
            super::classify(&body, &from("codex", None)),
            Some(HookMessage::Show { session_id: "codex:s1".into() })
        );
    }

    #[test]
    fn cursor_events_come_from_the_query_and_its_id_from_conversation_id() {
        let start = json!({ "hook_event_name": "beforeSubmitPrompt", "conversation_id": "c9" });
        assert_eq!(
            super::classify(&start, &from("cursor", Some("UserPromptSubmit"))),
            Some(HookMessage::Show { session_id: "cursor:c9".into() })
        );
        let stop = json!({ "hook_event_name": "stop", "conversation_id": "c9", "status": "completed" });
        assert_eq!(
            super::classify(&stop, &from("cursor", Some("Stop"))),
            Some(HookMessage::Stop { session_id: "cursor:c9".into() })
        );
    }

    #[test]
    fn antigravity_starts_only_on_the_first_model_call() {
        let q = from("antigravity", Some("PreInvocation"));
        let first = json!({ "conversationId": "g1", "invocationNum": 0 });
        let later = json!({ "conversationId": "g1", "invocationNum": 3 });
        assert_eq!(
            super::classify(&first, &q),
            Some(HookMessage::Show { session_id: "antigravity:g1".into() })
        );
        assert_eq!(super::classify(&later, &q), None);
        let stop = json!({ "conversationId": "g1", "terminationReason": "done" });
        assert_eq!(
            super::classify(&stop, &from("antigravity", Some("Stop"))),
            Some(HookMessage::Stop { session_id: "antigravity:g1".into() })
        );
    }

    #[test]
    fn the_query_event_works_with_an_empty_body() {
        assert_eq!(
            super::classify(&Value::Null, &from("cursor", Some("Stop"))),
            Some(HookMessage::Stop { session_id: "cursor:default".into() })
        );
    }

    #[test]
    fn tools_that_read_our_reply_get_what_they_expect() {
        assert_eq!(reply_for(&from("cursor", None)), json!({ "continue": true }));
        assert_eq!(reply_for(&from("antigravity", None)), json!({ "decision": "stop" }));
        assert_eq!(reply_for(&EventQuery::default()), json!({}));
    }

    #[test]
    fn a_browser_request_is_refused_even_with_our_host() {
        let mut h = headers_with_host("127.0.0.1:47913");
        assert!(!sent_by_a_browser(&h), "curl and hook runners send no Origin");
        h.insert(ORIGIN, "https://evil.example.com".parse().unwrap());
        assert!(sent_by_a_browser(&h));

        let mut h = headers_with_host("127.0.0.1:47913");
        h.insert("sec-fetch-site", "cross-site".parse().unwrap());
        assert!(sent_by_a_browser(&h));
    }
}
