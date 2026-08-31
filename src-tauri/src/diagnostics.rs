//! What the Status view needs to explain why games are, or are not, appearing.
//!
//! Every failure this app has is currently invisible: a port conflict, hooks
//! that were never installed, hooks left pointing at a port we moved off. In a
//! release build there is no console for the `eprintln!`s to land in, so the
//! user just sees an app that does nothing. This module is the answer to
//! "it's running but nothing happens".

use serde::Serialize;

use crate::logic::hooks;
use crate::server::HookMessage;
use crate::AppState;

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase", tag = "state")]
pub enum ServerStatus {
    /// Bound, or at least not yet known to have failed.
    Starting,
    Listening,
    Failed { reason: String },
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LastHookEvent {
    pub kind: String,
    pub session_id: String,
    pub at: String,
}

impl From<&HookMessage> for LastHookEvent {
    fn from(message: &HookMessage) -> Self {
        let (kind, session_id) = match message {
            HookMessage::Show { session_id } => ("thinking", session_id),
            HookMessage::Stop { session_id } => ("done", session_id),
        };
        Self {
            kind: kind.to_string(),
            session_id: session_id.clone(),
            at: chrono::Utc::now()
                .format("%Y-%m-%dT%H:%M:%S%.3fZ")
                .to_string(),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Diagnostics {
    pub server: ServerStatus,
    pub port: u16,
    pub hook_url: String,
    pub hooks_installed: bool,
    /// Present but unreadable — we refuse to touch it in that state.
    pub settings_readable: bool,
    pub settings_path: String,
    pub orphaned_hooks: Vec<String>,
    pub last_hook_event: Option<LastHookEvent>,
    pub show_delay_secs: u64,
    pub paused: bool,
}

pub fn snapshot(state: &AppState) -> Diagnostics {
    let settings = crate::tray::read_settings(&state.settings_path);
    let (hooks_installed, settings_readable, orphaned_hooks) = match &settings {
        Ok(value) => (
            hooks::is_installed(value, &state.hook_url),
            true,
            hooks::find_orphaned(value, &state.hook_url),
        ),
        Err(_) => (false, false, Vec::new()),
    };

    let prefs = state.prefs.lock().unwrap();
    Diagnostics {
        server: state.server_status.lock().unwrap().clone(),
        port: state.port,
        hook_url: state.hook_url.clone(),
        hooks_installed,
        settings_readable,
        settings_path: state.settings_path.display().to_string(),
        orphaned_hooks,
        last_hook_event: state.last_hook_event.lock().unwrap().clone(),
        show_delay_secs: prefs.show_delay_secs,
        paused: state
            .tracker
            .lock()
            .unwrap()
            .is_paused(std::time::Instant::now()),
    }
}
