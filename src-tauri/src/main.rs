// Release builds have no console window — this is a tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod logic;
mod server;
mod stats;
mod tray;
mod window;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use logic::tracker::{Action, Tracker};
use server::{HookMessage, DEFAULT_PORT};
use stats::Stats;

/// Keep this in sync with src/registry.ts — Rust picks the id, the frontend
/// mounts the matching module.
const GAMES: [&str; 3] = ["snake", "reaction", "typing"];

pub struct AppState {
    pub tracker: Mutex<Tracker>,
    pub stats: Mutex<Stats>,
    pub stats_path: PathBuf,
    pub settings_path: PathBuf,
    pub hook_url: String,
}

/// Turn a tracker decision into window and webview effects.
pub fn apply<R: Runtime>(app: &AppHandle<R>, action: Action) {
    match action {
        Action::Show { game_id } => {
            // Emit before showing so the first painted frame already has the
            // game in it rather than an empty stage.
            let _ = app.emit("game:show", json!({ "gameId": game_id }));
            show_window(app);
        }
        Action::Freeze => {
            // The frontend runs the score card and then calls hide_window.
            let _ = app.emit("game:freeze", ());
            watchdog(app);
        }
        Action::HideNow => hide_now(app),
        Action::Nothing => {}
    }
}

fn show_window<R: Runtime>(app: &AppHandle<R>) {
    window::show(app);
}

/// "Auto-dismissing" has to survive a broken webview. If the frontend never
/// calls `hide_window` after a freeze — crashed renderer, JS exception — hide
/// the window ourselves rather than leave it parked over the user's terminal.
fn watchdog<R: Runtime>(app: &AppHandle<R>) {
    const GRACE: Duration = Duration::from_secs(4);
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(GRACE).await;
        let stuck = {
            let state = app.state::<AppState>();
            let tracker = state.tracker.lock().unwrap();
            tracker.is_visible() && !tracker.has_active_sessions()
        };
        if stuck {
            eprintln!("[claudegame] frontend never dismissed the window; hiding it");
            hide_now(&app);
        }
    });
}

fn hide_now<R: Runtime>(app: &AppHandle<R>) {
    window::hide(app);
    let _ = app.emit("game:hide", ());
    if let Some(state) = app.try_state::<AppState>() {
        state.tracker.lock().unwrap().on_hidden();
    }
}

#[tauri::command]
fn get_stats(state: State<AppState>) -> Stats {
    state.stats.lock().unwrap().clone()
}

#[tauri::command]
fn record_result(
    state: State<AppState>,
    game_id: String,
    score: f64,
    duration_ms: f64,
    lower_is_better: bool,
) {
    let mut stats = state.stats.lock().unwrap();
    stats.record(&game_id, score, duration_ms, lower_is_better);
    if let Err(err) = stats.save(&state.stats_path) {
        eprintln!("[claudegame] could not save stats: {err}");
    }
}

/// Called by the frontend once the score card is done, and on Esc.
#[tauri::command]
fn hide_window(app: AppHandle) {
    hide_now(&app);
}

/// Asked once at webview boot. Tauri events are not replayed, so a hook that
/// lands while the webview is still loading would otherwise show an empty
/// window; this lets the frontend catch up.
#[tauri::command]
fn pending_game(state: State<AppState>) -> Option<String> {
    state.tracker.lock().unwrap().current_game()
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED)
}

fn port() -> u16 {
    std::env::var("CLAUDEGAME_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

fn main() {
    let port = port();
    let hook_url = format!("http://127.0.0.1:{port}/event");

    tauri::Builder::default()
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch just means "I want the tray app" — it is already
            // running, so nothing to do but say hello in the log.
            eprintln!("[claudegame] already running ({})", app.package_info().name);
        }))
        .invoke_handler(tauri::generate_handler![
            get_stats,
            record_result,
            hide_window,
            pending_game
        ])
        .setup(move |app| {
            let handle = app.handle().clone();

            // No Dock icon on macOS: this is a tray app, and an Accessory app
            // also hands focus back to the previous app when it hides.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let stats_path = handle
                .path()
                .app_data_dir()
                .expect("app data dir")
                .join("stats.json");
            let settings_path = handle
                .path()
                .home_dir()
                .expect("home dir")
                .join(".claude")
                .join("settings.json");

            app.manage(AppState {
                tracker: Mutex::new(Tracker::new(
                    GAMES.iter().map(|s| s.to_string()).collect(),
                    seed(),
                )),
                stats: Mutex::new(Stats::load(&stats_path)),
                stats_path,
                settings_path,
                hook_url: hook_url.clone(),
            });

            tray::build(&handle)?;

            let (tx, mut rx) = tokio::sync::mpsc::channel::<HookMessage>(32);

            // Hook endpoint.
            tauri::async_runtime::spawn(async move {
                if let Err(err) = server::serve(port, tx).await {
                    eprintln!("[claudegame] hook server failed on port {port}: {err}");
                }
            });

            // Single consumer of hook events — the tracker is only ever touched
            // from here and from tray/frontend commands.
            let event_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                while let Some(message) = rx.recv().await {
                    let now = Instant::now();
                    let state = event_handle.state::<AppState>();
                    let action = {
                        let mut tracker = state.tracker.lock().unwrap();
                        match message {
                            HookMessage::Show { session_id } => tracker.on_show_event(&session_id, now),
                            HookMessage::Stop { session_id } => tracker.on_stop_event(&session_id, now),
                        }
                    };
                    apply(&event_handle, action);
                }
            });

            // Sweep sessions whose terminal died without ever sending Stop.
            let tick_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(Duration::from_secs(60)).await;
                    let action = {
                        let state = tick_handle.state::<AppState>();
                        let mut tracker = state.tracker.lock().unwrap();
                        tracker.tick(Instant::now())
                    };
                    apply(&tick_handle, action);
                }
            });

            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to start claudegame")
        .run(|_app, event| {
            // The window closing must not quit the app — it lives in the tray.
            // An explicit exit (tray "Quit") carries a code, and must be let through.
            if let tauri::RunEvent::ExitRequested { api, code, .. } = event {
                if code.is_none() {
                    api.prevent_exit();
                }
            }
        });
}
