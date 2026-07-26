//! Tray icon, its menu, and the settings.json surgery behind "Enable hooks".

use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use serde_json::Value;
use tauri::menu::{Menu, MenuItem, PredefinedMenuItem, Submenu};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Manager, Runtime};

use crate::logic::hooks;
use crate::{apply, AppState};

pub const HOOKS_ITEM: &str = "hooks_toggle";

pub fn build<R: Runtime>(app: &AppHandle<R>) -> tauri::Result<()> {
    let play_now = MenuItem::with_id(app, "play_now", "Play a game now", true, None::<&str>)?;

    let pause_15 = MenuItem::with_id(app, "pause_15", "for 15 minutes", true, None::<&str>)?;
    let pause_60 = MenuItem::with_id(app, "pause_60", "for 1 hour", true, None::<&str>)?;
    let pause_forever = MenuItem::with_id(app, "pause_forever", "until I resume", true, None::<&str>)?;
    let resume = MenuItem::with_id(app, "resume", "Resume", true, None::<&str>)?;
    let pause = Submenu::with_id_and_items(
        app,
        "pause",
        "Pause",
        true,
        &[&pause_15, &pause_60, &pause_forever],
    )?;

    let hooks_item = MenuItem::with_id(app, HOOKS_ITEM, hooks_label(app), true, None::<&str>)?;
    let autostart = MenuItem::with_id(app, "autostart", autostart_label(app), true, None::<&str>)?;
    let open_stats = MenuItem::with_id(app, "open_stats", "Open stats file", true, None::<&str>)?;
    let quit = MenuItem::with_id(app, "quit", "Quit claudegame", true, None::<&str>)?;

    let menu = Menu::with_items(
        app,
        &[
            &play_now,
            &PredefinedMenuItem::separator(app)?,
            &pause,
            &resume,
            &PredefinedMenuItem::separator(app)?,
            &hooks_item,
            &autostart,
            &open_stats,
            &PredefinedMenuItem::separator(app)?,
            &quit,
        ],
    )?;

    TrayIconBuilder::with_id("claudegame")
        .icon(app.default_window_icon().expect("bundled icon").clone())
        .tooltip("claudegame — mini-games while Claude thinks")
        .menu(&menu)
        .on_menu_event(move |app, event| on_menu(app, event.id().as_ref()))
        .build(app)?;

    Ok(())
}

fn on_menu<R: Runtime>(app: &AppHandle<R>, id: &str) {
    let state = app.state::<AppState>();
    let now = Instant::now();

    let action = match id {
        "play_now" => state.tracker.lock().unwrap().force_show(now),
        "pause_15" => state
            .tracker
            .lock()
            .unwrap()
            .pause_for(Some(Duration::from_secs(15 * 60)), now),
        "pause_60" => state
            .tracker
            .lock()
            .unwrap()
            .pause_for(Some(Duration::from_secs(60 * 60)), now),
        "pause_forever" => state.tracker.lock().unwrap().pause_for(None, now),
        "resume" => {
            state.tracker.lock().unwrap().resume();
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
        "open_stats" => {
            reveal(&state.stats_path);
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

fn hooks_label<R: Runtime>(app: &AppHandle<R>) -> &'static str {
    let state = app.state::<AppState>();
    let settings = read_settings(&state.settings_path);
    if hooks::is_installed(&settings, &state.hook_url) {
        "Disable Claude Code hooks"
    } else {
        "Enable Claude Code hooks"
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
        eprintln!("[claudegame] could not change autostart: {err}");
    }
}

fn read_settings(path: &Path) -> Value {
    fs::read_to_string(path)
        .ok()
        .and_then(|raw| serde_json::from_str(&raw).ok())
        .unwrap_or_else(|| serde_json::json!({}))
}

/// Install or remove our hooks, keeping a one-time backup of the user's
/// original settings.json next to it.
fn toggle_hooks<R: Runtime>(app: &AppHandle<R>) {
    let state = app.state::<AppState>();
    let path = state.settings_path.clone();
    let mut settings = read_settings(&path);

    let installing = !hooks::is_installed(&settings, &state.hook_url);
    if installing {
        back_up(&path);
        hooks::install(&mut settings, &state.hook_url);
    } else {
        hooks::uninstall(&mut settings, &state.hook_url);
    }

    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    match serde_json::to_string_pretty(&settings)
        .map_err(|e| e.to_string())
        .and_then(|body| fs::write(&path, body + "\n").map_err(|e| e.to_string()))
    {
        Ok(()) => eprintln!(
            "[claudegame] hooks {} in {}",
            if installing { "installed" } else { "removed" },
            path.display()
        ),
        Err(err) => eprintln!("[claudegame] could not write {}: {err}", path.display()),
    }
}

fn back_up(path: &Path) {
    let backup = path.with_extension("json.claudegame-backup");
    if path.exists() && !backup.exists() {
        let _ = fs::copy(path, &backup);
    }
}

/// Show the stats file in the platform's file manager.
fn reveal(path: &PathBuf) {
    let path = path.clone();
    std::thread::spawn(move || {
        #[cfg(windows)]
        let result = std::process::Command::new("explorer")
            .arg("/select,")
            .arg(&path)
            .spawn();
        #[cfg(target_os = "macos")]
        let result = std::process::Command::new("open").arg("-R").arg(&path).spawn();
        #[cfg(all(unix, not(target_os = "macos")))]
        let result = std::process::Command::new("xdg-open")
            .arg(path.parent().unwrap_or(&path))
            .spawn();

        if let Err(err) = result {
            eprintln!("[claudegame] could not open stats location: {err}");
        }
    });
}
