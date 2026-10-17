//! Short-video feeds as the thing that opens while the agent thinks.
//!
//! Each site gets its own top-level window rather than an iframe in the game
//! window: YouTube, Instagram and TikTok all refuse to be framed. A top-level
//! `WebviewUrl::External` also sits outside the app's CSP, which stays as
//! strict as it is.
//!
//! Security: these windows load remote pages, and Tauri injects its IPC
//! internals into every webview. That is inert only because no capability
//! lists a `feed-*` label in `windows` and none has `remote.urls`. Keep it
//! that way — granting either hands the open web the app's commands.

use tauri::{AppHandle, Manager, Runtime, WebviewUrl, WebviewWindowBuilder, WindowEvent};
use tauri_plugin_opener::OpenerExt;

use crate::prefs::{FeedSite, FeedTarget};
use crate::window;

const LABEL_PREFIX: &str = "feed-";
const WIDTH: f64 = 800.0;
const HEIGHT: f64 = 1440.0;
const GAP: f64 = 12.0;

/// Pause every playing element and mark it, so resume only restarts what the
/// user was actually watching. Hiding a webview does not stop its audio.
const PAUSE_SCRIPT: &str = "document.querySelectorAll('video,audio').forEach(m=>{\
if(!m.paused){m.dataset.aaWasPlaying='1';m.pause();}})";
const RESUME_SCRIPT: &str = "document.querySelectorAll('[data-aa-was-playing]').forEach(m=>{\
delete m.dataset.aaWasPlaying;m.play().catch(()=>{});})";

impl FeedSite {
    pub const ALL: [FeedSite; 3] = [FeedSite::TikTok, FeedSite::Instagram, FeedSite::YouTubeShorts];

    fn label(self) -> &'static str {
        match self {
            FeedSite::TikTok => "feed-tiktok",
            FeedSite::Instagram => "feed-instagram",
            FeedSite::YouTubeShorts => "feed-youtube-shorts",
        }
    }

    fn title(self) -> &'static str {
        match self {
            FeedSite::TikTok => "TikTok",
            FeedSite::Instagram => "Instagram Reels",
            FeedSite::YouTubeShorts => "YouTube Shorts",
        }
    }

    fn url(self) -> &'static str {
        match self {
            FeedSite::TikTok => "https://www.tiktok.com/foryou",
            FeedSite::Instagram => "https://www.instagram.com/reels/",
            FeedSite::YouTubeShorts => "https://www.youtube.com/shorts",
        }
    }
}

/// Open the chosen feeds, in app windows or in the user's browser.
pub fn show_all<R: Runtime>(app: &AppHandle<R>, sites: &[FeedSite], target: FeedTarget) {
    // Once per bout, before anything of ours takes the foreground.
    window::remember_foreground();

    match target {
        FeedTarget::DefaultBrowser => {
            for site in sites {
                if let Err(err) = app.opener().open_url(site.url(), None::<&str>) {
                    alog!("could not open {}: {err}", site.url());
                }
            }
        }
        FeedTarget::AppWindows => {
            // Windows for sites the user has since unticked stay hidden.
            hide_all(app);
            let slots = tile(app, sites.len());
            for (site, (x, y, w, h)) in sites.iter().zip(slots) {
                show_window(app, *site, x, y, w, h);
            }
            if let Some(first) = sites.first().and_then(|s| app.get_webview_window(s.label())) {
                let _ = first.set_focus();
            }
        }
    }
}

/// Pause and hide every feed window. Focus is handed back by the caller,
/// which hides the game window through `window::hide` straight after.
pub fn hide_all<R: Runtime>(app: &AppHandle<R>) {
    for (label, win) in app.webview_windows() {
        if label.starts_with(LABEL_PREFIX) {
            let _ = win.eval(PAUSE_SCRIPT);
            let _ = win.hide();
        }
    }
}

fn show_window<R: Runtime>(app: &AppHandle<R>, site: FeedSite, x: f64, y: f64, w: f64, h: f64) {
    if let Some(existing) = app.get_webview_window(site.label()) {
        // Left where the user put it last time; only the first open is tiled.
        let _ = existing.show();
        let _ = existing.unminimize();
        let _ = existing.eval(RESUME_SCRIPT);
        return;
    }

    let Ok(url) = site.url().parse() else {
        return;
    };
    let built = WebviewWindowBuilder::new(app, site.label(), WebviewUrl::External(url))
        .title(site.title())
        .inner_size(w, h)
        .position(x, y)
        .resizable(true)
        .decorations(true)
        .always_on_top(true)
        .build();

    match built {
        Ok(win) => {
            // Closing a feed dismisses the bout, the way Esc does for a game.
            // The window is kept, so its login and scroll position survive.
            let handle = app.clone();
            win.on_window_event(move |event| {
                if let WindowEvent::CloseRequested { api, .. } = event {
                    api.prevent_close();
                    crate::hide_now(&handle);
                }
            });
        }
        Err(err) => alog!("could not open the {} window: {err}", site.title()),
    }
}

/// Side-by-side slots, centred in the primary monitor's work area (the screen
/// minus the taskbar or Dock), in logical pixels.
fn tile<R: Runtime>(app: &AppHandle<R>, count: usize) -> Vec<(f64, f64, f64, f64)> {
    let (mx, my, mw, mh) = app
        .primary_monitor()
        .ok()
        .flatten()
        .map(|m| {
            let scale = m.scale_factor();
            let area = m.work_area();
            let pos = area.position.to_logical::<f64>(scale);
            let size = area.size.to_logical::<f64>(scale);
            (pos.x, pos.y, size.width, size.height)
        })
        .unwrap_or((0.0, 0.0, 1280.0, 800.0));
    layout(count, mx, my, mw, mh)
}

/// Pure half of `tile`, so the geometry is testable without a display.
fn layout(count: usize, mx: f64, my: f64, mw: f64, mh: f64) -> Vec<(f64, f64, f64, f64)> {
    if count == 0 {
        return Vec::new();
    }
    let n = count as f64;
    // Shrink to fit rather than spill off-screen on a small display.
    // Height first, then width at the same ratio, so a short screen keeps the
    // reel's tall shape instead of a squat window with bars beside the video.
    let h = HEIGHT.min(mh - 2.0 * GAP - 60.0).max(360.0);
    let w = (h * WIDTH / HEIGHT).min((mw - GAP * (n + 1.0)) / n).max(240.0);
    let total = w * n + GAP * (n - 1.0);
    let left = mx + ((mw - total) / 2.0).max(0.0);
    let top = my + ((mh - h) / 2.0).max(0.0);
    (0..count)
        .map(|i| (left + i as f64 * (w + GAP), top, w, h))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_label_carries_the_prefix() {
        for site in FeedSite::ALL {
            assert!(site.label().starts_with(LABEL_PREFIX));
        }
    }

    #[test]
    fn every_url_is_https() {
        for site in FeedSite::ALL {
            assert!(site.url().starts_with("https://"), "{}", site.url());
        }
    }

    #[test]
    fn three_windows_fit_side_by_side_on_a_laptop() {
        let slots = layout(3, 0.0, 0.0, 1280.0, 800.0);
        assert_eq!(slots.len(), 3);
        let (lx, _, _, _) = slots[0];
        let (rx, _, rw, _) = slots[2];
        assert!(lx >= 0.0 && rx + rw <= 1280.0, "{slots:?}");
        assert!(slots[0].0 + slots[0].2 < slots[1].0, "windows must not overlap");
    }

    #[test]
    fn one_reel_on_a_1440p_screen_is_large_and_keeps_its_shape() {
        let (_, _, w, h) = layout(1, 0.0, 0.0, 2560.0, 1392.0)[0];
        assert!(h > 1200.0, "{h}");
        assert!((w / h - WIDTH / HEIGHT).abs() < 0.01, "{w}x{h}");
    }

    #[test]
    fn a_narrow_screen_shrinks_the_windows() {
        let slots = layout(3, 0.0, 0.0, 900.0, 700.0);
        assert!(slots[0].2 < WIDTH);
        assert!(slots[0].3 < HEIGHT);
    }

    #[test]
    fn a_secondary_origin_is_respected() {
        let slots = layout(1, 1920.0, 0.0, 1920.0, 1080.0);
        assert!(slots[0].0 >= 1920.0);
    }
}
