//! Showing and hiding the game window, and giving the terminal its focus back.
//!
//! The games need keystrokes, so showing the window takes focus. That debt is
//! repaid on hide: on Windows we remember which window was in front before we
//! barged in and put it back, otherwise the user is left typing into nothing.

use tauri::{AppHandle, Manager, Runtime};

pub const MAIN_WINDOW: &str = "main";

/// What the remembered-window slot should become on a `show`.
///
/// `candidate` is `None` when the foreground window is not a window we could
/// ever hand focus back to — it is invalid, or it belongs to us. In that case
/// keep whatever we already had: overwriting the slot with one of our own
/// windows means `restore` "returns" focus to ourselves and the user is left
/// typing into nothing.
pub(crate) fn next_previous(current: Option<isize>, candidate: Option<isize>) -> Option<isize> {
    candidate.or(current)
}

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
    use windows::Win32::System::Threading::GetCurrentProcessId;
    use windows::Win32::UI::WindowsAndMessaging::{
        GetForegroundWindow, GetWindowThreadProcessId, IsWindow, SetForegroundWindow,
    };

    use super::next_previous;

    // isize rather than HWND so the value is Send across the mutex.
    static PREVIOUS: Mutex<Option<isize>> = Mutex::new(None);

    /// Does this window belong to us? Tauri leaves the hidden main window as the
    /// foreground window immediately after startup, so on the first game of a
    /// session `GetForegroundWindow` hands us our own window.
    fn is_ours(hwnd: HWND) -> bool {
        let mut pid = 0u32;
        unsafe { GetWindowThreadProcessId(hwnd, Some(&mut pid)) };
        pid != 0 && pid == unsafe { GetCurrentProcessId() }
    }

    pub fn remember() {
        let hwnd = unsafe { GetForegroundWindow() };
        let candidate = (!hwnd.is_invalid() && !is_ours(hwnd)).then(|| hwnd.0 as isize);
        let mut previous = PREVIOUS.lock().unwrap();
        *previous = next_previous(*previous, candidate);
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

#[cfg(test)]
mod tests {
    use super::next_previous;

    #[test]
    fn a_real_window_is_remembered() {
        assert_eq!(next_previous(None, Some(0x1234)), Some(0x1234));
    }

    #[test]
    fn a_newer_window_replaces_an_older_one() {
        assert_eq!(next_previous(Some(0x1111), Some(0x2222)), Some(0x2222));
    }

    /// The cold-start case: our own hidden window is foreground, so there is no
    /// candidate. Remembering it would make `restore` hand focus to ourselves.
    #[test]
    fn our_own_window_never_becomes_the_target() {
        assert_eq!(next_previous(None, None), None);
    }

    #[test]
    fn our_own_window_does_not_evict_a_good_target() {
        assert_eq!(next_previous(Some(0x1111), None), Some(0x1111));
    }
}

fn restore_foreground() {
    platform::restore();
}
