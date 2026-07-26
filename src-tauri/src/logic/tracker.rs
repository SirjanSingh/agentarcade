//! All of the app's show/hide behaviour, with no Tauri types in sight.
//!
//! Every rule that decides whether a game appears lives here so it can be
//! tested with `cargo test` and no window: the 2s debounce that defuses Claude
//! Code's terminal-flicker bug, the per-session ref-count that stops one
//! session's `Stop` from closing a game another session is still keeping open,
//! and the stale sweep that stops a killed terminal pinning the window open.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::picker::{choose_excluding, Rng};

/// Two shows closer together than this are the flicker bug, not the user.
pub const SHOW_DEBOUNCE: Duration = Duration::from_millis(2000);
/// A session that has gone quiet this long is assumed dead (terminal killed).
pub const SESSION_STALE_AFTER: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Show the window and start this game.
    Show { game_id: String },
    /// Claude is done: stop the run, show the score card, then hide.
    Freeze,
    /// Hide immediately without a score card (pause, quit).
    HideNow,
    Nothing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Pause {
    Until(Instant),
    Indefinitely,
}

pub struct Tracker {
    games: Vec<String>,
    /// session_id -> time of that session's last event.
    active: HashMap<String, Instant>,
    visible: bool,
    last_show: Option<Instant>,
    last_game: Option<String>,
    pause: Option<Pause>,
    rng: Rng,
}

impl Tracker {
    pub fn new(games: Vec<String>, seed: u64) -> Self {
        Self {
            games,
            active: HashMap::new(),
            visible: false,
            last_show: None,
            last_game: None,
            pause: None,
            rng: Rng::new(seed),
        }
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// True while at least one Claude Code session is still thinking.
    pub fn has_active_sessions(&self) -> bool {
        !self.active.is_empty()
    }

    /// The game that should be on screen right now, if any. Lets a webview that
    /// missed the `game:show` event (still loading when the hook arrived) pick
    /// the run up on boot instead of showing an empty stage.
    pub fn current_game(&self) -> Option<String> {
        if self.visible {
            self.last_game.clone()
        } else {
            None
        }
    }

    pub fn is_paused(&self, now: Instant) -> bool {
        match self.pause {
            Some(Pause::Indefinitely) => true,
            Some(Pause::Until(t)) => now < t,
            None => false,
        }
    }

    /// `UserPromptSubmit` or `PreCompact`: Claude started thinking.
    pub fn on_show_event(&mut self, session_id: &str, now: Instant) -> Action {
        self.active.insert(session_id.to_string(), now);

        if self.is_paused(now) {
            return Action::Nothing;
        }
        // Already playing — never restart a run mid-game.
        if self.visible {
            return Action::Nothing;
        }
        // The flicker bug fires several UserPromptSubmit hooks in quick
        // succession; only the first one gets a game.
        if let Some(last) = self.last_show {
            if now.duration_since(last) < SHOW_DEBOUNCE {
                return Action::Nothing;
            }
        }
        self.start_game(now)
    }

    /// `Stop`: this session finished. Only the last one out turns off the lights.
    pub fn on_stop_event(&mut self, session_id: &str, now: Instant) -> Action {
        self.active.remove(session_id);
        self.sweep_stale(now);

        if !self.active.is_empty() {
            return Action::Nothing;
        }
        if self.visible {
            Action::Freeze
        } else {
            Action::Nothing
        }
    }

    /// Periodic tick. Drops sessions whose terminal died without a `Stop`.
    pub fn tick(&mut self, now: Instant) -> Action {
        let had = !self.active.is_empty();
        self.sweep_stale(now);
        if had && self.active.is_empty() && self.visible {
            return Action::Freeze;
        }
        Action::Nothing
    }

    /// Tray "Play now" — deliberately ignores both pause and debounce.
    pub fn force_show(&mut self, now: Instant) -> Action {
        if self.visible {
            return Action::Nothing;
        }
        self.start_game(now)
    }

    pub fn pause_for(&mut self, duration: Option<Duration>, now: Instant) -> Action {
        self.pause = Some(match duration {
            Some(d) => Pause::Until(now + d),
            None => Pause::Indefinitely,
        });
        if self.visible {
            Action::HideNow
        } else {
            Action::Nothing
        }
    }

    pub fn resume(&mut self) {
        self.pause = None;
    }

    /// Called once the window is actually hidden (score card done, or Esc).
    pub fn on_hidden(&mut self) {
        self.visible = false;
    }

    fn start_game(&mut self, now: Instant) -> Action {
        let roll = self.rng.next_u64();
        match choose_excluding(&self.games, self.last_game.as_deref(), roll) {
            Some(game_id) => {
                let game_id = game_id.to_string();
                self.visible = true;
                self.last_show = Some(now);
                self.last_game = Some(game_id.clone());
                Action::Show { game_id }
            }
            None => Action::Nothing,
        }
    }

    fn sweep_stale(&mut self, now: Instant) {
        self.active
            .retain(|_, seen| now.duration_since(*seen) < SESSION_STALE_AFTER);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn tracker() -> Tracker {
        Tracker::new(
            vec!["snake".into(), "reaction".into(), "typing".into()],
            1234,
        )
    }

    fn show(action: &Action) -> bool {
        matches!(action, Action::Show { .. })
    }

    #[test]
    fn first_prompt_shows_a_game() {
        let mut t = tracker();
        assert!(show(&t.on_show_event("a", Instant::now())));
        assert!(t.is_visible());
    }

    #[test]
    fn flicker_burst_yields_exactly_one_game() {
        // The bug this whole debounce exists for: Claude Code's terminal
        // flicker fires UserPromptSubmit several times in a few hundred ms.
        let mut t = tracker();
        let t0 = Instant::now();
        let mut shows = 0;
        for i in 0..6 {
            if show(&t.on_show_event("a", t0 + Duration::from_millis(i * 100))) {
                shows += 1;
            }
        }
        assert_eq!(shows, 1, "flicker burst opened {shows} games");
    }

    #[test]
    fn a_later_prompt_shows_again_once_hidden() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        assert_eq!(t.on_stop_event("a", t0 + Duration::from_millis(500)), Action::Freeze);
        t.on_hidden();
        assert!(show(&t.on_show_event("a", t0 + Duration::from_millis(2100))));
    }

    #[test]
    fn debounce_still_applies_after_an_early_dismissal() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        t.on_hidden(); // user hit Esc straight away
        assert_eq!(
            t.on_show_event("a", t0 + Duration::from_millis(300)),
            Action::Nothing
        );
    }

    #[test]
    fn a_show_while_playing_does_not_restart_the_run() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        assert_eq!(
            t.on_show_event("a", t0 + Duration::from_secs(30)),
            Action::Nothing
        );
    }

    #[test]
    fn one_sessions_stop_does_not_close_anothers_game() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        // Session b starts thinking while the game from a is on screen.
        assert_eq!(t.on_show_event("b", t0 + Duration::from_secs(1)), Action::Nothing);

        assert_eq!(
            t.on_stop_event("a", t0 + Duration::from_secs(2)),
            Action::Nothing,
            "session b is still thinking — the window must stay"
        );
        assert_eq!(t.on_stop_event("b", t0 + Duration::from_secs(3)), Action::Freeze);
    }

    #[test]
    fn stop_from_an_unknown_session_does_not_hide() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        assert_eq!(
            t.on_stop_event("ghost", t0 + Duration::from_secs(1)),
            Action::Nothing
        );
    }

    #[test]
    fn stop_with_nothing_showing_is_a_no_op() {
        let mut t = tracker();
        assert_eq!(t.on_stop_event("a", Instant::now()), Action::Nothing);
    }

    #[test]
    fn a_dead_session_is_swept_and_releases_the_window() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        // Terminal killed: no Stop ever arrives.
        assert_eq!(t.tick(t0 + Duration::from_secs(60)), Action::Nothing);
        assert_eq!(t.tick(t0 + SESSION_STALE_AFTER + Duration::from_secs(1)), Action::Freeze);
    }

    #[test]
    fn tick_is_idle_when_nothing_is_active() {
        let mut t = tracker();
        assert_eq!(t.tick(Instant::now()), Action::Nothing);
    }

    #[test]
    fn pausing_blocks_shows_but_stop_still_hides() {
        let mut t = tracker();
        let t0 = Instant::now();
        assert!(show(&t.on_show_event("a", t0)));
        assert_eq!(t.pause_for(Some(Duration::from_secs(900)), t0), Action::HideNow);
        t.on_hidden();

        assert_eq!(t.on_show_event("a", t0 + Duration::from_secs(10)), Action::Nothing);
        assert!(t.is_paused(t0 + Duration::from_secs(899)));
        assert!(!t.is_paused(t0 + Duration::from_secs(901)));
        assert!(show(&t.on_show_event("a", t0 + Duration::from_secs(901))));
    }

    #[test]
    fn indefinite_pause_never_expires_until_resumed() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.pause_for(None, t0);
        assert!(t.is_paused(t0 + Duration::from_secs(86_400)));
        t.resume();
        assert!(!t.is_paused(t0));
        assert!(show(&t.on_show_event("a", t0)));
    }

    #[test]
    fn force_show_ignores_pause_and_debounce() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.pause_for(None, t0);
        assert!(show(&t.force_show(t0)));
        t.on_hidden();
        assert!(show(&t.force_show(t0 + Duration::from_millis(10))));
    }

    #[test]
    fn consecutive_games_differ() {
        let mut t = tracker();
        let mut previous: Option<String> = None;
        let t0 = Instant::now();
        for i in 0..25 {
            let now = t0 + Duration::from_secs(i * 10);
            let action = t.on_show_event("a", now);
            let Action::Show { game_id } = action else {
                panic!("expected a game at iteration {i}, got {action:?}");
            };
            assert_ne!(Some(&game_id), previous.as_ref());
            previous = Some(game_id);
            t.on_stop_event("a", now + Duration::from_secs(1));
            t.on_hidden();
        }
    }
}
