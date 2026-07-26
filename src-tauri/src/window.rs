//! Showing and hiding the game window, and giving the terminal its focus back.
//!
//! The games need keystrokes, so showing the window takes focus. That debt is
//! repaid on hide: on Windows we remember which window was in front before we
//! barged in and put it back, otherwise the user is left typing into nothing.

use tauri::{AppHandle, Manager, Runtime};

pub const MAIN_WINDOW: &str = "main";

pub fn show<R: Runtime>(app: &AppHandle<R>) {
    remember_foreground();
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.show();
        let _ = window.set_always_on_top(true);
        let _ = window.set_focus();
    }
}

pub fn hide<R: Runtime>(app: &AppHandle<R>) {
    if let Some(window) = app.get_webview_window(MAIN_WINDOW) {
        let _ = window.hide();
    }
    restore_foreground();
}

#[cfg(windows)]
mod platform {
    use std::sync::Mutex;
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{GetForegroundWindow, IsWindow, SetForegroundWindow};

    // isize rather than HWND so the value is Send across the mutex.
    static PREVIOUS: Mutex<Option<isize>> = Mutex::new(None);

    pub fn remember() {
        let hwnd = unsafe { GetForegroundWindow() };
        if !hwnd.is_invalid() {
            *PREVIOUS.lock().unwrap() = Some(hwnd.0 as isize);
        }
    }

    pub fn restore() {
        let Some(raw) = PREVIOUS.lock().unwrap().take() else {
            return;
        };
        let hwnd = HWND(raw as *mut std::ffi::c_void);
        unsafe {
            // The terminal may have been closed while we had the window up.
            if IsWindow(Some(hwnd)).as_bool() {
                let _ = SetForegroundWindow(hwnd);
            }
        }
    }
}

#[cfg(not(windows))]
mod platform {
    // macOS gets this for free from the Accessory activation policy: hiding a
    // non-activating app returns focus to whatever was underneath. On Linux it
    // is the window manager's call, and there is no portable way to insist.
    pub fn remember() {}
    pub fn restore() {}
}

fn remember_foreground() {
    platform::remember();
}

fn restore_foreground() {
    platform::restore();
}
