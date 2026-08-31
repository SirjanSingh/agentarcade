//! Tray icon, its menu, and the settings.json surgery behind "Enable hooks".

use std::fs;
use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::menu::{CheckMenuItem, Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Emitter, Manager, Runtime};

use crate::logic::hooks;
use crate::prefs::SHOW_DELAY_CHOICES;
use crate::{apply, AppState};

pub const HOOKS_ITEM: &str = "hooks_toggle";

fn delay_item_id(secs: u64) -> String {
    format!("delay_{secs}")
}

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let play_now = MenuItem::with_id(app, "play_now", "Play a game now", true, None::<&str>)?;

    let pause_15 = MenuItem::with_id(app, "pause_15", "for 15 minutes", true, None::<&str>)?;
    let pause_60 = MenuItem::with_id(app, "pause_60", "for 1 hour", true, None::<&str>)?;
    let pause_forever =
        MenuItem::with_id(app, "pause_forever", "until I resume", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume", true, None::<&str>)?;
    let pause = Submenu::with_id_and_items(
        app,
        "pause",
        "Pause",
        true,
        &[&pause_15, &pause_60, &pause_forever],
    )?;

    // How long a session has to be thinking before a game is worth showing.
    // The whole point of the app is to fill a wait, so a wait that never
    // happened must not be interrupted.
    let current_delay = app.state::<AppState>().prefs.lock().unwrap().show_delay_secs;
    let delay_items: Vec<CheckMenuItem<R>> = SHOW_DELAY_CHOICES
        .iter()
        .map(|secs| {
            CheckMenuItem::with_id(
                app,
                delay_item_id(*secs),
                format!("after {secs} seconds"),
                true,
                *secs == current_delay,
                None::<&str>,
            )
        })
        .collect::<tauri::Result<_>>()?;
    let delay_refs: Vec<&dyn tauri::menu::IsMenuItem<R>> = delay_items
        .iter()
        .map(|i| i as &dyn tauri::menu::IsMenuItem<R>)
        .collect();
    let delay = Submenu::with_id_and_items(app, "delay", "Show a game", true, &delay_refs)?;

    let hooks_item = MenuItem::with_id(app, HOOKS_ITEM, hooks_label(app), true, None::<&str>)?;
    let autostart = MenuItem::with_id(app, "autostart", autostart_label(app), true, None::<&str>)?;
    let settings = MenuItem::with_id(app, "settings", "Settings & status…", true, None::<&str>)?;
    let show_stats = MenuItem::with_id(app, "show_stats", "Show stats", true, None::<&str>)?;
    let open_stats = MenuItem::with_id(app, "open_stats", "Open stats file", true, None::<&str>)?;
    let check_updates =
        MenuItem::with_id(app, "check_updates", "Check for updates…", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit agentarcade", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &play_now,
            &show_stats,
            &PredefinedMenuItem::separator(app)?,
            &delay,
            &pause,
            &resume,
            &PredefinedMenuItem::separator(app)?,
            &hooks_item,
            &autostart,
            &settings,
            &open_stats,
            &check_updates,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("agentarcade")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("agentarcade — mini-games while your agent thinks")
        .menu(&menu)
        .on_menu_event(move |app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let state = app.state::<AppState>();
    let now = Instant::now();

    // Delay choices are dynamic ids, so they are matched before the fixed set.
    if let Some(secs) = SHOW_DELAY_CHOICES
        .iter()
        .find(|s| delay_item_id(**s) == id)
        .copied()
    {
        set_show_delay(app, secs);
        return;
    }

    let action = match id {
        "play_now" => state.tracker.lock().unwrap().force_show(now),
        "pause_15" => pause_for(app, Some(Duration::from_secs(15 * 60)), now),
        "pause_60" => pause_for(app, Some(Duration::from_secs(60 * 60)), now),
        "pause_forever" => pause_for(app, None, now),
        "resume" => {
            state.tracker.lock().unwrap().resume();
            state.prefs.lock().unwrap().pause = None;
            state.save_prefs();
            crate::logic::tracker::Action::Nothing
        }
        HOOKS_ITEM => {
            toggle_hooks(app);
            crate::logic::tracker::Action::Nothing
        }
        "autostart" => {
            toggle_autostart(app);
            crate::logic::tracker::Action::Nothing
        }
        "settings" => {
            crate::window::open_settings(app, "status");
            crate::logic::tracker::Action::Nothing
        }
        "show_stats" => {
            // Opened by hand, so it is dismissed by hand (Esc) — the tracker
            // is deliberately not told the window is showing a game.
            let _ = app.emit("view:stats", ());
            crate::window::show(app);
            crate::logic::tracker::Action::Nothing
        }
        "open_stats" => {
            reveal(&state.stats_path);
            crate::logic::tracker::Action::Nothing
        }
        "check_updates" => {
            check_for_updates(app);
            crate::logic::tracker::Action::Nothing
        }
        "quit" => {
            app.exit(0);
            return;
        }
        _ => crate::logic::tracker::Action::Nothing,
    };

    apply(app, action);
}

/// Pause, and persist the deadline so it survives a restart. A 15-minute pause
/// that forgets itself the moment the app reopens is not a pause.
fn pause_for<R: Runtime>(
    app: &AppHandle<R>,
    duration: Option<Duration>,
    now: Instant,
) -> crate::logic::tracker::Action {
    let state = app.state::<AppState>();
    let action = state.tracker.lock().unwrap().pause_for(duration, now);
    state
        .prefs
        .lock()
        .unwrap()
        .set_pause(duration, crate::prefs::now_epoch_millis());
    state.save_prefs();
    action
}

fn set_show_delay<R: Runtime>(app: &AppHandle<R>, secs: u64) {
    let state = app.state::<AppState>();
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
    eprintln!("[agentarcade] show delay set to {secs}s");
}

fn hooks_label<R: Runtime>(app: &AppHandle<R>) -> &'static str {
    let state = app.state::<AppState>();
    match read_settings(&state.settings_path) {
        Ok(settings) if hooks::is_installed(&settings, &state.hook_url) => "Disable agent hooks",
        Ok(_) => "Enable agent hooks",
        // Neither label is honest when we cannot read the file, and guessing
        // "Enable" invites a click that would overwrite it.
        Err(_) => "Agent hooks — settings.json unreadable",
    }
}

fn autostart_label<R: Runtime>(app: &AppHandle<R>) -> &'static str {
    use tauri_plugin_autostart::ManagerExt;
    if app.autolaunch().is_enabled().unwrap_or(false) {
        "Don't start at login"
    } else {
        "Start at login"
    }
}

/// Worth having: hooks that fire while the app is not running are the one way
/// this whole thing silently does nothing.
fn toggle_autostart<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_autostart::ManagerExt;
    let manager = app.autolaunch();
    let result = if manager.is_enabled().unwrap_or(false) {
        manager.disable()
    } else {
        manager.enable()
    };
    if let Err(err) = result {
        eprintln!("[agentarcade] could not change autostart: {err}");
    }
}

/// Read the user's settings.json.
///
/// A missing file is fine — that is a user who has never configured anything.
/// A file that exists but does not parse is *not* fine, and must not be treated
/// as an empty object: doing so would silently replace whatever they had with
/// our hooks and nothing else. Callers are expected to refuse to write.
pub fn read_settings(path: &Path) -> Result<Value, String> {
    let raw = match fs::read_to_string(path) {
        Ok(raw) => raw,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => {
            return Ok(serde_json::json!({}))
        }
        Err(err) => return Err(format!("could not read {}: {err}", path.display())),
    };
    if raw.trim().is_empty() {
        return Ok(serde_json::json!({}));
    }
    serde_json::from_str(&raw).map_err(|err| format!("{} is not valid JSON: {err}", path.display()))
}

fn write_settings(path: &Path, settings: &Value) -> Result<(), String> {
    let mut body = serde_json::to_string_pretty(settings).map_err(|e| e.to_string())?;
    body.push('\n');
    // Atomic: this is the user's own editor config, and a half-written one is
    // far worse than a failed write.
    crate::atomic_write::write(path, body.as_bytes())
        .map_err(|err| format!("could not write {}: {err}", path.display()))
}

fn toggle_hooks<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let installed = match read_settings(&state.settings_path) {
        Ok(settings) => hooks::is_installed(&settings, &state.hook_url),
        Err(err) => {
            report(app, &err);
            return;
        }
    };
    if let Err(err) = set_hooks(&state, !installed) {
        report(app, &err);
    }
}

/// Install or remove our hooks, keeping a one-time backup of the user's
/// original settings.json next to it.
///
/// Refuses to touch a settings.json it cannot parse — see `read_settings`.
pub fn set_hooks(state: &AppState, install: bool) -> Result<(), String> {
    let path = state.settings_path.clone();
    let mut settings = read_settings(&path)?;

    if install {
        back_up(&path);
        // A port change orphans the previous entry, which then costs every
        // prompt a 2s timeout for a listener that is not there.
        if let Some(previous) = state.prefs.lock().unwrap().last_hook_url.clone() {
            if previous != state.hook_url {
                hooks::uninstall(&mut settings, &previous);
            }
        }
        hooks::install(&mut settings, &state.hook_url);
    } else {
        hooks::uninstall(&mut settings, &state.hook_url);
    }

    write_settings(&path, &settings)?;

    {
        let mut prefs = state.prefs.lock().unwrap();
        prefs.last_hook_url = install.then(|| state.hook_url.clone());
        // Installing hooks is the one step setup actually requires. Someone who
        // did it and then closed the window rather than clicking Done has
        // finished, and should not be met by the welcome screen every launch.
        if install {
            prefs.first_run_complete = true;
        }
    }
    state.save_prefs();

    eprintln!(
        "[agentarcade] hooks {} in {}",
        if install { "installed" } else { "removed" },
        path.display()
    );
    Ok(())
}

/// Drop hook entries of ours that point at a port we are not on.
pub fn clean_orphaned_hooks(state: &AppState) -> Result<usize, String> {
    let path = state.settings_path.clone();
    let mut settings = read_settings(&path)?;
    let removed = hooks::remove_orphaned(&mut settings, &state.hook_url);
    if removed > 0 {
        back_up(&path);
        write_settings(&path, &settings)?;
    }
    Ok(removed)
}

/// Say it on screen, not just to a stderr a release build does not have.
/// Ask GitHub whether there is a newer release, and install it if so.
///
/// Deliberately manual rather than a check on every launch: this is a tray
/// toy, and a background process that phones out on its own would contradict
/// the "no network calls" promise in the README. The only outbound request in
/// the binary is the one the user asks for by clicking this.
///
/// The downloaded artifact is verified against the public key compiled into
/// the binary before anything is written, so a compromised release host still
/// cannot ship a payload we would install.
fn check_for_updates<R: Runtime>(app: &AppHandle<R>) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    use tauri_plugin_updater::UpdaterExt;

    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let updater = match app.updater() {
            Ok(updater) => updater,
            Err(err) => return notify(&app, &format!("Could not start the updater: {err}"), true),
        };

        match updater.check().await {
            Ok(Some(update)) => {
                let version = update.version.clone();
                match update.download_and_install(|_, _| {}, || {}).await {
                    Ok(()) => {
                        app.dialog()
                            .message(format!(
                                "agentarcade {version} is installed.                                  The app will restart to finish."
                            ))
                            .title("Update installed")
                            .kind(MessageDialogKind::Info)
                            .blocking_show();
                        app.restart();
                    }
                    Err(err) => notify(&app, &format!("The update failed to install: {err}"), true),
                }
            }
            Ok(None) => notify(&app, "You are on the latest version.", false),
            Err(err) => notify(&app, &format!("Could not check for updates: {err}"), true),
        }
    });
}

fn notify<R: Runtime>(app: &AppHandle<R>, message: &str, is_error: bool) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    eprintln!("[agentarcade] {message}");
    let app = app.clone();
    let message = message.to_string();
    std::thread::spawn(move || {
        app.dialog()
            .message(message)
            .title("agentarcade")
            .kind(if is_error {
                MessageDialogKind::Error
            } else {
                MessageDialogKind::Info
            })
            .blocking_show();
    });
}

fn report<R: Runtime>(app: &AppHandle<R>, message: &str) {
    use tauri_plugin_dialog::{DialogExt, MessageDialogKind};
    eprintln!("[agentarcade] {message}");
    let app = app.clone();
    let message = message.to_string();
    std::thread::spawn(move || {
        app.dialog()
            .message(format!(
                "{message}\n\nagentarcade will not modify the file until it is valid JSON \
                 again, so nothing has been changed."
            ))
            .title("Could not update agent hooks")
            .kind(MessageDialogKind::Error)
            .blocking_show();
    });
}

fn back_up(path: &Path) {
    let backup = path.with_extension("json.agentarcade-backup");
    if path.exists() && !backup.exists() {
        let _ = fs::copy(path, &backup);
    }
}

/// Show the stats file in the platform's file manager.
fn reveal(path: &Path) {
    let path = path.to_path_buf();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let result = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn();
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open")
            .arg("-R")
            .arg(&path)
            .spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(&path))
            .spawn();

        if let Err(err) = result {
            eprintln!("[agentarcade] could not open stats location: {err}");
        }
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agentarcade-tray-{name}"));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir.join("settings.json")
    }

    #[test]
    fn a_missing_settings_file_reads_as_empty() {
        let path = temp_path("missing").with_file_name("nope.json");
        assert_eq!(read_settings(&path).unwrap(), serde_json::json!({}));
    }

    #[test]
    fn an_empty_settings_file_reads_as_empty() {
        let path = temp_path("blank");
        fs::write(&path, "   \n").unwrap();
        assert_eq!(read_settings(&path).unwrap(), serde_json::json!({}));
    }

    #[test]
    fn a_malformed_settings_file_is_an_error_not_an_empty_object() {
        // The regression that matters: returning {} here would have silently
        // replaced the user's real config on the next hook toggle.
        let path = temp_path("malformed");
        fs::write(&path, r#"{ "model": "opus",, }"#).unwrap();
        assert!(read_settings(&path).is_err());
    }

    #[test]
    fn writing_settings_round_trips() {
        let path = temp_path("write");
        let value = serde_json::json!({ "model": "opus" });
        write_settings(&path, &value).unwrap();
        assert_eq!(read_settings(&path).unwrap(), value);
        assert!(
            fs::read_to_string(&path).unwrap().ends_with('\n'),
            "settings.json should keep a trailing newline"
        );
    }

    #[test]
    fn backup_is_taken_once_and_never_overwritten() {
        let path = temp_path("backup");
        fs::write(&path, r#"{"original":true}"#).unwrap();
        back_up(&path);
        fs::write(&path, r#"{"changed":true}"#).unwrap();
        back_up(&path);
        let backup = path.with_extension("json.agentarcade-backup");
        assert_eq!(
            fs::read_to_string(backup).unwrap(),
            r#"{"original":true}"#,
            "the first backup must not be clobbered by a later one"
        );
    }
}
