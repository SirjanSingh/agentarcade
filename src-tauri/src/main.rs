// Release builds have no console window — this is a tray app.
#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

mod atomic_write;
mod diagnostics;
mod feeds;
mod logic;
mod maker;
mod packs;
mod prefs;
mod server;
mod stats;
mod tray;
mod window;

use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde_json::json;
use tauri::{AppHandle, Emitter, Manager, Runtime, State};

use diagnostics::{Diagnostics, LastHookEvent, ServerStatus};
use logic::games;
use logic::tracker::{Action, Tracker};
use prefs::{FeedSite, FeedTarget, Prefs, ThinkingContent, FEEDS_ID};
use server::{HookMessage, DEFAULT_PORT};
use stats::Stats;

/// How often the tracker is asked to reconsider.
///
/// This used to be 60s when its only job was sweeping dead sessions. The show
/// delay is now decided here too, so the tick is load-bearing for timing: at
/// 500ms a 10s delay lands within half a second of where the user asked for it.
/// The cost is a mutex lock and a scan of a handful of map entries twice a
/// second, which is nothing.
const TICK: Duration = Duration::from_millis(500);

pub struct AppState {
    pub tracker: Mutex<Tracker>,
    pub stats: Mutex<Stats>,
    pub prefs: Mutex<Prefs>,
    pub stats_path: PathBuf,
    pub prefs_path: PathBuf,
    pub settings_path: PathBuf,
    /// Where generated game packs live. Scanned rather than cached, so a game
    /// the user just asked their agent for turns up without a restart.
    pub packs_dir: PathBuf,
    /// Where the game-maker skill is installed, if the user asks for it.
    pub skill_dir: PathBuf,
    pub hook_url: String,
    pub port: u16,
    /// Whether the hook endpoint actually bound. Without this a port conflict
    /// is invisible: the tray icon appears, games never do, and the reason only
    /// ever reached a stderr that release builds do not have.
    pub server_status: Mutex<ServerStatus>,
    pub last_hook_event: Mutex<Option<LastHookEvent>>,
}

impl AppState {
    /// Persist prefs, logging rather than propagating — a failed preference
    /// write must never take down the action the user asked for.
    pub fn save_prefs(&self) {
        let prefs = self.prefs.lock().unwrap();
        if let Err(err) = prefs.save(&self.prefs_path) {
            eprintln!("[agentarcade] could not save prefs: {err}");
        }
    }
}

/// Turn a tracker decision into window and webview effects.
pub fn apply<R: Runtime>(app: &AppHandle<R>, action: Action) {
    match action {
        Action::Show { game_id } if game_id == FEEDS_ID => {
            let (sites, target) = {
                let state = app.state::<AppState>();
                let prefs = state.prefs.lock().unwrap();
                (prefs.feed_sites.clone(), prefs.feed_target)
            };
            feeds::show_all(app, &sites, target);
        }
        Action::Show { game_id } => {
            // Emit before showing so the first painted frame already has the
            // game in it rather than an empty stage.
            let _ = app.emit("game:show", json!({ "gameId": game_id }));
            show_window(app);
        }
        // A feed has no score card to run, so there is nothing to wait for.
        Action::Freeze if showing_feeds(app) => hide_now(app),
        Action::Freeze => {
            // The frontend runs the score card and then calls hide_window.
            let _ = app.emit("game:freeze", ());
            watchdog(app);
        }
        Action::HideNow => hide_now(app),
        Action::Nothing => {}
    }
}

fn showing_feeds<R: Runtime>(app: &AppHandle<R>) -> bool {
    let state = app.state::<AppState>();
    let current = state.tracker.lock().unwrap().current_game();
    current.as_deref() == Some(FEEDS_ID)
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
            eprintln!("[agentarcade] frontend never dismissed the window; hiding it");
            hide_now(&app);
        }
    });
}

fn hide_now<R: Runtime>(app: &AppHandle<R>) {
    // Feeds first: window::hide is what hands focus back to the terminal.
    feeds::hide_all(app);
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
        eprintln!("[agentarcade] could not save stats: {err}");
    }
}

/// Called by the frontend once the score card is done, and on Esc.
#[tauri::command]
fn hide_window(app: AppHandle) {
    hide_now(&app);
}

/// Built-in game ids plus every generated pack currently on disk.
///
/// Rescanned on every call rather than cached. The scan is a handful of small
/// reads of a directory that holds a few entries, and caching it would mean a
/// game the user just asked their agent for stayed invisible until the next
/// launch — which is the one thing this feature must not do.
fn all_game_ids(state: &AppState) -> Vec<String> {
    let builtins = games::game_ids();
    let mut ids = builtins.clone();
    ids.extend(
        packs::scan(&state.packs_dir, &builtins)
            .into_iter()
            .map(|pack| pack.id),
    );
    ids
}

/// Rebuild the rotation pool from disk. Called after anything that can change
/// which games exist or which of them are switched on.
///
/// Takes no lock of its own for longer than it must: the scan happens before
/// either mutex is touched, so a slow disk cannot stall a hook event.
fn refresh_pool(state: &AppState) {
    let all = all_game_ids(state);
    let pool = state.prefs.lock().unwrap().rotation(&all);
    state.tracker.lock().unwrap().set_games(pool);
}

/// Asked once at webview boot. Tauri events are not replayed, so a hook that
/// lands while the webview is still loading would otherwise show an empty
/// window; this lets the frontend catch up.
#[tauri::command]
fn pending_game(state: State<AppState>) -> Option<String> {
    // The feeds are not a game; the game webview must not try to mount them.
    state
        .tracker
        .lock()
        .unwrap()
        .current_game()
        .filter(|id| id != FEEDS_ID)
}

/// The registered games plus which of them the user has switched off.
#[tauri::command]
fn get_prefs(state: State<AppState>) -> serde_json::Value {
    let prefs = state.prefs.lock().unwrap();
    json!({
        "showDelaySecs": prefs.show_delay_secs,
        "scorecardDurationMs": prefs.scorecard_duration_ms,
        "disabledGames": prefs.disabled_games,
        "firstRunComplete": prefs.first_run_complete,
        "thinkingContent": prefs.thinking_content,
        "feedSites": prefs.feed_sites,
        "feedTarget": prefs.feed_target,
        // Generated packs are included so the Options view can switch them off
        // exactly like a built-in game, with no extra machinery.
        "allGames": all_game_ids(&state),
    })
}

/// Every generated pack the app can currently load. The Games view lists these.
#[tauri::command]
fn list_packs(state: State<AppState>) -> Vec<packs::Pack> {
    packs::scan(&state.packs_dir, &games::game_ids())
}

#[tauri::command]
fn delete_pack(state: State<AppState>, id: String) -> Result<(), String> {
    packs::delete(&state.packs_dir, &id)?;
    // The deleted game may have been in the rotation pool; drop it before the
    // tracker can pick an id that no longer resolves to anything.
    refresh_pool(&state);
    Ok(())
}

/// Whether the game-maker skill is installed, absent, or blocked by a folder
/// of the same name that we did not write.
#[tauri::command]
fn maker_skill_status(state: State<AppState>) -> maker::SkillStatus {
    maker::status(&state.skill_dir)
}

#[tauri::command]
fn set_maker_skill_installed(state: State<AppState>, installed: bool) -> Result<(), String> {
    if installed {
        maker::install(&state.skill_dir)
    } else {
        maker::remove(&state.skill_dir)
    }
}

#[tauri::command]
fn set_show_delay(state: State<AppState>, secs: u64) {
    {
        let mut prefs = state.prefs.lock().unwrap();
        prefs.show_delay_secs = secs;
        state
            .tracker
            .lock()
            .unwrap()
            .set_show_delay(prefs.show_delay());
    }
    state.save_prefs();
}

#[tauri::command]
fn set_game_enabled(state: State<AppState>, game_id: String, enabled: bool) {
    {
        let mut prefs = state.prefs.lock().unwrap();
        prefs.disabled_games.retain(|id| id != &game_id);
        if !enabled {
            prefs.disabled_games.push(game_id);
        }
    }
    refresh_pool(&state);
    state.save_prefs();
}

/// After a feed setting changes: rebuild the pool, save, and if what is on
/// screen no longer matches the setting, put it away rather than leave a stale
/// window (or a game when feeds were asked for) up until the agent stops.
fn after_feed_change(app: &AppHandle, target_changed: bool) {
    let state = app.state::<AppState>();
    refresh_pool(&state);
    state.save_prefs();
    let wants_feeds = state.prefs.lock().unwrap().shows_feeds();
    let current = state.tracker.lock().unwrap().current_game();
    if let Some(current) = current {
        let has_feeds = current == FEEDS_ID;
        if has_feeds != wants_feeds || (has_feeds && target_changed) {
            hide_now(app);
        }
    }
}

#[tauri::command]
fn set_thinking_content(app: AppHandle, content: ThinkingContent) {
    app.state::<AppState>().prefs.lock().unwrap().thinking_content = content;
    after_feed_change(&app, false);
}

#[tauri::command]
fn set_feed_enabled(app: AppHandle, site: FeedSite, enabled: bool) {
    {
        let state = app.state::<AppState>();
        let mut prefs = state.prefs.lock().unwrap();
        prefs.feed_sites.retain(|s| *s != site);
        if enabled {
            prefs.feed_sites.push(site);
            // Keep a stable left-to-right order however they were ticked.
            prefs
                .feed_sites
                .sort_by_key(|s| FeedSite::ALL.iter().position(|a| a == s));
        }
    }
    after_feed_change(&app, false);
}

#[tauri::command]
fn set_feed_target(app: AppHandle, target: FeedTarget) {
    app.state::<AppState>().prefs.lock().unwrap().feed_target = target;
    after_feed_change(&app, true);
}

#[tauri::command]
fn complete_onboarding(state: State<AppState>) {
    state.prefs.lock().unwrap().first_run_complete = true;
    state.save_prefs();
}

/// Everything the Status view needs to explain why games are or are not
/// appearing, without the user reading a log they cannot reach.
#[tauri::command]
fn diagnostics_snapshot(state: State<AppState>) -> Diagnostics {
    diagnostics::snapshot(&state)
}

#[tauri::command]
fn remove_orphaned_hooks(state: State<AppState>) -> Result<usize, String> {
    tray::clean_orphaned_hooks(&state)
}

#[tauri::command]
fn set_hooks_installed(state: State<AppState>, installed: bool) -> Result<(), String> {
    tray::set_hooks(&state, installed)
}

#[tauri::command]
fn play_now(app: AppHandle) {
    let action = {
        let state = app.state::<AppState>();
        let mut tracker = state.tracker.lock().unwrap();
        tracker.force_show(Instant::now())
    };
    apply(&app, action);
}

#[tauri::command]
fn open_settings(app: AppHandle, view: Option<String>) {
    window::open_settings(&app, view.as_deref().unwrap_or("status"));
}

/// What the pack protocol answers with for anything it cannot resolve. No body
/// and no detail: a miss here is either a deleted game or someone probing, and
/// neither is owed an explanation.
fn empty_404() -> tauri::http::Response<Vec<u8>> {
    tauri::http::Response::builder()
        .status(404)
        .body(Vec::new())
        .expect("a 404 with an empty body is always well-formed")
}

fn seed() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos() as u64)
        .unwrap_or(0x5EED)
}

fn port() -> u16 {
    std::env::var("AGENTARCADE_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(DEFAULT_PORT)
}

/// Fail loudly rather than silently.
///
/// A release build has no console (`windows_subsystem = "windows"`), so an
/// `.expect()` here would look to the user like the app simply never started.
/// Write the reason somewhere retrievable, then say so on screen.
fn fatal(app: &AppHandle, msg: &str) -> ! {
    eprintln!("[agentarcade] fatal: {msg}");
    let log = std::env::temp_dir().join("agentarcade-startup-error.log");
    let _ = std::fs::write(&log, format!("agentarcade could not start.\n\n{msg}\n"));

    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    app.dialog()
        .message(format!(
            "{msg}\n\nDetails were written to:\n{}",
            log.display()
        ))
        .title("agentarcade could not start")
        .kind(MessageDialogKind::Error)
        .blocking_show();
    std::process::exit(1);
}

fn main() {
    let port = port();
    let hook_url = format!("http://127.0.0.1:{port}/event");

    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        // Rust-side only, for feeds in the default browser. No capability
        // grants it to any webview.
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_process::init())
        .plugin(tauri_plugin_autostart::init(
            tauri_plugin_autostart::MacosLauncher::LaunchAgent,
            None,
        ))
        .plugin(tauri_plugin_single_instance::init(|app, _argv, _cwd| {
            // A second launch just means "I want the tray app" — it is already
            // running, so surface the settings window rather than doing nothing.
            eprintln!("[agentarcade] already running ({})", app.package_info().name);
            window::open_settings(app, "status");
        }))
        .invoke_handler(tauri::generate_handler![
            get_stats,
            record_result,
            hide_window,
            pending_game,
            get_prefs,
            set_show_delay,
            set_game_enabled,
            set_thinking_content,
            set_feed_enabled,
            set_feed_target,
            complete_onboarding,
            diagnostics_snapshot,
            remove_orphaned_hooks,
            set_hooks_installed,
            play_now,
            open_settings,
            list_packs,
            delete_pack,
            maker_skill_status,
            set_maker_skill_installed
        ])
        // Generated games are served from here rather than inlined into the
        // page. A `srcdoc` or `blob:` frame inherits this window's CSP, which
        // forbids inline script, so a generated game simply would not run —
        // and loosening the main window's script-src to fix that would defeat
        // the point. Its own scheme gives the frame its own origin and, below,
        // its own policy, while the app's stays exactly as strict as it is.
        .register_uri_scheme_protocol(packs::SCHEME, |ctx, request| {
            let state = ctx.app_handle().state::<AppState>();
            let response = tauri::http::Response::builder();

            // The id arrives from the webview, so it is validated here even
            // though the scan that produced it validated it too.
            let id = request.uri().path().trim_start_matches('/');
            let id = id.strip_suffix("/game.html").unwrap_or("");

            match packs::html_path(&state.packs_dir, id).and_then(|p| std::fs::read(p).ok()) {
                Some(bytes) => response
                    .header(tauri::http::header::CONTENT_TYPE, "text/html; charset=utf-8")
                    // The layer that keeps "nothing leaves your machine" true.
                    // A generated fetch() or a remote <script> dies here even
                    // if the model writes one; the sandbox attribute on the
                    // iframe cannot express this on its own.
                    .header(
                        tauri::http::header::CONTENT_SECURITY_POLICY,
                        "default-src 'none'; script-src 'unsafe-inline'; \
                         style-src 'unsafe-inline'; img-src data:",
                    )
                    .body(bytes)
                    .unwrap_or_else(|_| empty_404()),
                None => empty_404(),
            }
        })
        .setup(move |app| {
            let handle = app.handle().clone();

            // No Dock icon on macOS: this is a tray app, and an Accessory app
            // also hands focus back to the previous app when it hides.
            #[cfg(target_os = "macos")]
            app.set_activation_policy(tauri::ActivationPolicy::Accessory);

            let Ok(data_dir) = handle.path().app_data_dir() else {
                fatal(
                    &handle,
                    "Windows did not give this app a data directory, so settings and stats \
                     cannot be saved.",
                );
            };
            let Ok(home) = handle.path().home_dir() else {
                fatal(
                    &handle,
                    "Your home directory could not be found, so agent hooks cannot be installed.",
                );
            };

            let stats_path = data_dir.join("stats.json");
            let prefs_path = data_dir.join("prefs.json");
            let packs_dir = data_dir.join("games");
            let settings_path = home.join(".claude").join("settings.json");
            let skill_dir = home.join(".claude").join("skills").join(maker::SKILL_DIR);

            let stats = Stats::load(&stats_path);
            let prefs = Prefs::load(&prefs_path);

            let builtins = games::game_ids();
            let mut all_games = builtins.clone();
            all_games.extend(
                packs::scan(&packs_dir, &builtins)
                    .into_iter()
                    .map(|pack| pack.id),
            );
            let mut tracker = Tracker::new(
                prefs.rotation(&all_games),
                prefs.show_delay(),
                seed(),
            );
            // A pause that was running at shutdown keeps its remaining time.
            tracker.restore_pause(prefs.pause_at(Instant::now(), prefs::now_epoch_millis()));

            let first_run = !prefs.first_run_complete;

            app.manage(AppState {
                tracker: Mutex::new(tracker),
                stats: Mutex::new(stats),
                prefs: Mutex::new(prefs),
                stats_path,
                prefs_path,
                settings_path,
                packs_dir,
                skill_dir,
                hook_url: hook_url.clone(),
                port,
                server_status: Mutex::new(ServerStatus::Starting),
                last_hook_event: Mutex::new(None),
            });

            tray::build(&handle)?;

            let (tx, mut rx) = tokio::sync::mpsc::channel::<HookMessage>(32);

            // Hook endpoint. A bind failure is recorded in state rather than
            // only printed, so the Status view can explain the silence.
            let server_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                match server::serve(port, tx).await {
                    Ok(()) => {}
                    Err(err) => {
                        let reason = err.to_string();
                        eprintln!("[agentarcade] hook server failed on port {port}: {reason}");
                        let state = server_handle.state::<AppState>();
                        *state.server_status.lock().unwrap() = ServerStatus::Failed { reason };
                    }
                }
            });
            // The listener binds before serve() starts yielding, so anything
            // still Starting a moment later is Listening.
            let ready_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                tokio::time::sleep(Duration::from_millis(400)).await;
                let state = ready_handle.state::<AppState>();
                let mut status = state.server_status.lock().unwrap();
                if matches!(*status, ServerStatus::Starting) {
                    *status = ServerStatus::Listening;
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
                        match &message {
                            HookMessage::Show { session_id } => {
                                tracker.on_show_event(session_id, now)
                            }
                            HookMessage::Stop { session_id } => {
                                tracker.on_stop_event(session_id, now)
                            }
                        }
                    };
                    *state.last_hook_event.lock().unwrap() = Some(LastHookEvent::from(&message));
                    apply(&event_handle, action);
                }
            });

            // Decides delayed shows, and sweeps sessions whose terminal died
            // without ever sending Stop.
            let tick_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                // Packs appear while the app is running: the user asks their
                // agent for a game and expects to play it. Scanning on every
                // 500ms tick would be a lot of pointless directory reads, and
                // waiting for a restart would make a freshly generated game
                // look broken. Ten seconds sits between the two.
                const PACK_RESCAN_TICKS: u32 = 20;
                let mut ticks: u32 = 0;
                let mut known: Vec<String> = {
                    let state = tick_handle.state::<AppState>();
                    all_game_ids(&state)
                };

                loop {
                    tokio::time::sleep(TICK).await;
                    let action = {
                        let state = tick_handle.state::<AppState>();
                        let mut tracker = state.tracker.lock().unwrap();
                        tracker.tick(Instant::now())
                    };
                    apply(&tick_handle, action);

                    ticks = ticks.wrapping_add(1);
                    if ticks % PACK_RESCAN_TICKS == 0 {
                        let state = tick_handle.state::<AppState>();
                        let current = all_game_ids(&state);
                        if current != known {
                            known = current;
                            refresh_pool(&state);
                            // The game window is long-lived, so it has to be
                            // told; it cannot pick this up at boot.
                            let _ = tick_handle.emit("packs:changed", ());
                        }
                    }
                }
            });

            if first_run {
                window::open_settings(&handle, "welcome");
            }

            Ok(())
        })
        .build(tauri::generate_context!())
        // Runs before any window exists, so there is nowhere to show a dialog.
        // Tauri's own bootstrap failing is both rare and unrecoverable.
        .expect("failed to start agentarcade")
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
