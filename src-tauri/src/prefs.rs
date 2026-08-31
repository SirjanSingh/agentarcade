//! User preferences, file-backed next to `stats.json`.
//!
//! Shape and conventions mirror `stats.rs` (camelCase, a `version` field, a
//! corrupt file preserved as `.bak` rather than silently discarded) so the two
//! files behave identically from the user's point of view.
//!
//! Every field is `#[serde(default)]` so a `prefs.json` written by an older
//! build — or one a user hand-edited down to `{}` — still loads.

use std::path::Path;
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::logic::tracker::Pause;

const VERSION: u32 = 1;

/// The delays the tray offers. Anything else in the file is clamped to the
/// nearest of these on load, so a hand-edited `prefs.json` cannot put the app
/// into a state the tray has no radio button for.
pub const SHOW_DELAY_CHOICES: [u64; 4] = [5, 10, 30, 60];

pub const DEFAULT_SHOW_DELAY_SECS: u64 = 10;
/// Matches SCORECARD_MS in src/main.ts.
pub const DEFAULT_SCORECARD_MS: u64 = 1500;

fn default_show_delay_secs() -> u64 {
    DEFAULT_SHOW_DELAY_SECS
}

fn default_scorecard_ms() -> u64 {
    DEFAULT_SCORECARD_MS
}

/// A pause, stored as wall-clock rather than as an `Instant`.
///
/// `Instant` is monotonic and process-local: it has no meaning across a
/// restart. A 15-minute pause that was active at shutdown has to come back as
/// "13 minutes left", not as "expired" or "starts over now", so the deadline is
/// persisted as epoch millis and converted back to an `Instant` on load.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", tag = "kind")]
pub enum PausePref {
    Until { epoch_millis: u64 },
    Indefinite,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Prefs {
    #[serde(default)]
    pub version: u32,
    /// Seconds a session must think continuously before a game appears.
    #[serde(default = "default_show_delay_secs")]
    pub show_delay_secs: u64,
    #[serde(default)]
    pub pause: Option<PausePref>,
    #[serde(default = "default_scorecard_ms")]
    pub scorecard_duration_ms: u64,
    /// The hook URL we last wrote into the user's settings, so a port change
    /// can clean up the entry it orphaned. See logic::hooks::find_orphaned.
    #[serde(default)]
    pub last_hook_url: Option<String>,
    #[serde(default)]
    pub first_run_complete: bool,
    /// Games the user has switched off. An *exclusion* list, so a fresh install
    /// with no prefs file plays everything and needs no migration.
    #[serde(default)]
    pub disabled_games: Vec<String>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            version: VERSION,
            show_delay_secs: DEFAULT_SHOW_DELAY_SECS,
            pause: None,
            scorecard_duration_ms: DEFAULT_SCORECARD_MS,
            last_hook_url: None,
            first_run_complete: false,
            disabled_games: Vec::new(),
        }
    }
}

impl Prefs {
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = std::fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Prefs>(&raw) {
            Ok(mut prefs) => {
                prefs.version = VERSION;
                prefs.show_delay_secs = nearest_choice(prefs.show_delay_secs);
                prefs
            }
            Err(err) => {
                eprintln!("[agentarcade] prefs.json unreadable ({err}); using defaults");
                let _ = std::fs::write(path.with_extension("json.bak"), raw);
                Self::default()
            }
        }
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::atomic_write::write(path, &serde_json::to_vec_pretty(self)?)
    }

    pub fn show_delay(&self) -> Duration {
        Duration::from_secs(self.show_delay_secs)
    }

    /// The games actually eligible for the picker.
    ///
    /// Falls back to the full list if the user has somehow disabled every game
    /// — an empty rotation would mean the app silently stops working, which is
    /// worse than ignoring the preference.
    pub fn enabled_games(&self, all: &[String]) -> Vec<String> {
        let enabled: Vec<String> = all
            .iter()
            .filter(|id| !self.disabled_games.contains(id))
            .cloned()
            .collect();
        if enabled.is_empty() {
            all.to_vec()
        } else {
            enabled
        }
    }

    /// Rebuild the in-memory pause from the persisted deadline.
    ///
    /// Returns `None` for a pause that expired while the app was closed.
    pub fn pause_at(&self, now: Instant, now_epoch_millis: u64) -> Option<Pause> {
        match self.pause? {
            PausePref::Indefinite => Some(Pause::Indefinitely),
            PausePref::Until { epoch_millis } => {
                let remaining = epoch_millis.checked_sub(now_epoch_millis)?;
                if remaining == 0 {
                    None
                } else {
                    Some(Pause::Until(now + Duration::from_millis(remaining)))
                }
            }
        }
    }

    /// Record a pause that should survive a restart.
    pub fn set_pause(&mut self, duration: Option<Duration>, now_epoch_millis: u64) {
        self.pause = Some(match duration {
            Some(d) => PausePref::Until {
                epoch_millis: now_epoch_millis + d.as_millis() as u64,
            },
            None => PausePref::Indefinite,
        });
    }
}

pub fn now_epoch_millis() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as u64)
        .unwrap_or(0)
}

/// Snap an arbitrary delay to the nearest value the tray can display.
fn nearest_choice(secs: u64) -> u64 {
    SHOW_DELAY_CHOICES
        .iter()
        .copied()
        .min_by_key(|c| c.abs_diff(secs))
        .unwrap_or(DEFAULT_SHOW_DELAY_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_path(name: &str) -> std::path::PathBuf {
        let dir = std::env::temp_dir().join(format!("agentarcade-prefs-{name}"));
        let _ = std::fs::remove_dir_all(&dir);
        dir.join("prefs.json")
    }

    #[test]
    fn defaults_are_sane() {
        let p = Prefs::default();
        assert_eq!(p.show_delay_secs, 10);
        assert_eq!(p.scorecard_duration_ms, 1500);
        assert!(!p.first_run_complete);
        assert!(p.disabled_games.is_empty());
    }

    #[test]
    fn missing_file_loads_defaults() {
        let p = Prefs::load(&temp_path("missing"));
        assert_eq!(p.show_delay_secs, DEFAULT_SHOW_DELAY_SECS);
    }

    #[test]
    fn save_then_load_round_trips() {
        let path = temp_path("round-trip");
        let p = Prefs {
            show_delay_secs: 30,
            first_run_complete: true,
            disabled_games: vec!["snake".into()],
            ..Prefs::default()
        };
        p.save(&path).unwrap();

        let back = Prefs::load(&path);
        assert_eq!(back.show_delay_secs, 30);
        assert!(back.first_run_complete);
        assert_eq!(back.disabled_games, vec!["snake".to_string()]);
    }

    #[test]
    fn an_empty_object_loads_every_default() {
        let path = temp_path("empty");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{}").unwrap();
        let p = Prefs::load(&path);
        assert_eq!(p.show_delay_secs, DEFAULT_SHOW_DELAY_SECS);
        assert_eq!(p.scorecard_duration_ms, DEFAULT_SCORECARD_MS);
        assert!(
            !path.with_extension("json.bak").exists(),
            "an empty object is valid prefs, not a corrupt file"
        );
    }

    #[test]
    fn a_partial_file_keeps_the_fields_it_does_have() {
        let path = temp_path("partial");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"firstRunComplete": true}"#).unwrap();
        let p = Prefs::load(&path);
        assert!(p.first_run_complete, "a partial file must not fall back wholesale");
        assert_eq!(p.show_delay_secs, DEFAULT_SHOW_DELAY_SECS);
    }

    #[test]
    fn a_corrupt_file_is_kept_as_bak() {
        let path = temp_path("corrupt");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, "{ not json").unwrap();
        let p = Prefs::load(&path);
        assert_eq!(p.show_delay_secs, DEFAULT_SHOW_DELAY_SECS);
        assert!(path.with_extension("json.bak").exists());
    }

    #[test]
    fn a_hand_edited_delay_is_snapped_to_a_tray_choice() {
        let path = temp_path("snap");
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, r#"{"showDelaySecs": 27}"#).unwrap();
        assert_eq!(Prefs::load(&path).show_delay_secs, 30);
    }

    #[test]
    fn pause_survives_a_restart_across_the_epoch_boundary() {
        // Paused for 15 minutes, then the app is closed and reopened 5 minutes
        // later: 10 minutes must still be on the clock.
        let mut p = Prefs::default();
        let paused_at = 1_700_000_000_000u64;
        p.set_pause(Some(Duration::from_secs(900)), paused_at);

        let restarted_at = paused_at + 300_000;
        let now = Instant::now();
        let pause = p.pause_at(now, restarted_at).expect("pause still running");
        match pause {
            Pause::Until(t) => {
                let left = t.duration_since(now);
                assert!(
                    left.as_secs() >= 599 && left.as_secs() <= 600,
                    "expected ~600s left, got {left:?}"
                );
            }
            other => panic!("expected a timed pause, got {other:?}"),
        }
    }

    #[test]
    fn a_pause_that_expired_while_closed_is_dropped() {
        let mut p = Prefs::default();
        let paused_at = 1_700_000_000_000u64;
        p.set_pause(Some(Duration::from_secs(900)), paused_at);
        // Reopened an hour later.
        assert!(p.pause_at(Instant::now(), paused_at + 3_600_000).is_none());
    }

    #[test]
    fn an_indefinite_pause_survives_any_gap() {
        let mut p = Prefs::default();
        p.set_pause(None, 0);
        assert_eq!(
            p.pause_at(Instant::now(), 9_999_999_999_999),
            Some(Pause::Indefinitely)
        );
    }

    #[test]
    fn disabled_games_are_excluded() {
        let all: Vec<String> = vec!["snake".into(), "aim".into(), "typing".into()];
        let p = Prefs {
            disabled_games: vec!["aim".into()],
            ..Prefs::default()
        };
        assert_eq!(p.enabled_games(&all), vec!["snake".to_string(), "typing".into()]);
    }

    #[test]
    fn disabling_every_game_falls_back_to_all_of_them() {
        let all: Vec<String> = vec!["snake".into(), "aim".into()];
        let p = Prefs {
            disabled_games: all.clone(),
            ..Prefs::default()
        };
        assert_eq!(p.enabled_games(&all), all, "an empty rotation would break the app");
    }
}
