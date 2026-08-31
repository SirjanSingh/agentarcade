//! All of the app's show/hide behaviour, with no Tauri types in sight.
//!
//! Every rule that decides whether a game appears lives here so it can be
//! tested with `cargo test` and no window: the elapsed-time delay that keeps a
//! three-second answer from popping a game at all, the per-session ref-count
//! that stops one session's `Stop` from closing a game another session is still
//! keeping open, and the stale sweep that stops a killed terminal pinning the
//! window open.
//!
//! The delay is why `on_show_event` no longer shows anything. A hook event only
//! records that a session *started* thinking; `tick` decides, later, whether it
//! has been thinking long enough to be worth interrupting. A session that
//! finishes before its delay elapses never opens a window at all.

use std::collections::HashMap;
use std::time::{Duration, Instant};

use super::picker::{choose_excluding, Rng};

/// Two shows closer together than this are the flicker bug, not the user.
///
/// With a minimum configurable delay of 5s this almost never fires any more —
/// the delay itself absorbs the flicker. It is kept as a cheap guard for a
/// show/dismiss/show landing exactly on the delay boundary.
pub const SHOW_DEBOUNCE: Duration = Duration::from_millis(2000);
/// A session that has gone quiet this long is assumed dead (terminal killed).
pub const SESSION_STALE_AFTER: Duration = Duration::from_secs(600);

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Action {
    /// Show the window and start this game.
    Show { game_id: String },
    /// The agent is done: stop the run, show the score card, then hide.
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
    show_delay: Duration,
    /// session_id -> time of that session's last event.
    active: HashMap<String, Instant>,
    /// session_id -> when this thinking bout began. Distinct from `active`,
    /// which is refreshed on every event: this one is set once per bout and is
    /// what the delay is measured against.
    pending_since: HashMap<String, Instant>,
    visible: bool,
    last_show: Option<Instant>,
    last_game: Option<String>,
    pause: Option<Pause>,
    rng: Rng,
}

impl Tracker {
    pub fn new(games: Vec<String>, show_delay: Duration, seed: u64) -> Self {
        Self {
            games,
            show_delay,
            active: HashMap::new(),
            pending_since: HashMap::new(),
            visible: false,
            last_show: None,
            last_game: None,
            pause: None,
            rng: Rng::new(seed),
        }
    }

    pub fn set_show_delay(&mut self, delay: Duration) {
        self.show_delay = delay;
    }

    pub fn set_games(&mut self, games: Vec<String>) {
        self.games = games;
    }

    /// Reinstate a pause that was persisted across a restart.
    pub fn restore_pause(&mut self, pause: Option<Pause>) {
        self.pause = pause;
    }

    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// True while at least one agent session is still thinking.
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

    /// `UserPromptSubmit` or `PreCompact`: this session started thinking.
    ///
    /// Deliberately never shows a game. It only starts the clock — `tick` is
    /// what decides the session has been thinking long enough. This is the
    /// whole fix for "a three-second answer pops a game".
    pub fn on_show_event(&mut self, session_id: &str, now: Instant) -> Action {
        self.active.insert(session_id.to_string(), now);
        // or_insert, not insert: a flicker burst of repeat events must not keep
        // pushing the start of the bout forward.
        self.pending_since.entry(session_id.to_string()).or_insert(now);
        Action::Nothing
    }

    /// `Stop`: this session finished. Only the last one out turns off the lights.
    pub fn on_stop_event(&mut self, session_id: &str, now: Instant) -> Action {
        self.active.remove(session_id);
        self.pending_since.remove(session_id);
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

    /// Periodic tick. Drops sessions whose terminal died without a `Stop`, and
    /// opens a game once a session has been thinking for longer than the delay.
    pub fn tick(&mut self, now: Instant) -> Action {
        let had = !self.active.is_empty();
        self.sweep_stale(now);
        if had && self.active.is_empty() && self.visible {
            return Action::Freeze;
        }
        if self.visible || self.is_paused(now) {
            return Action::Nothing;
        }
        let waited_long_enough = self
            .pending_since
            .values()
            .any(|since| now.duration_since(*since) >= self.show_delay);
        if !waited_long_enough {
            return Action::Nothing;
        }
        if let Some(last) = self.last_show {
            if now.duration_since(last) < SHOW_DEBOUNCE {
                return Action::Nothing;
            }
        }
        self.start_game(now)
    }

    /// Tray "Play now" — deliberately ignores pause, delay and debounce.
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
                // Showing consumes every pending bout. Without this, dismissing
                // with Esc while a session is still thinking would re-open a
                // game on the very next tick.
                self.pending_since.clear();
                Action::Show { game_id }
            }
            None => Action::Nothing,
        }
    }

    fn sweep_stale(&mut self, now: Instant) {
        self.active
            .retain(|_, seen| now.duration_since(*seen) < SESSION_STALE_AFTER);
        self.pending_since
            .retain(|id, _| self.active.contains_key(id));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const DELAY: Duration = Duration::from_secs(10);

    fn games() -> Vec<String> {
        vec![
            "snake".into(),
            "reaction".into(),
            "typing".into(),
            "aim".into(),
            "gradient".into(),
            "boulder".into(),
        ]
    }

    fn tracker() -> Tracker {
        Tracker::new(games(), DELAY, 1234)
    }

    fn show(action: &Action) -> bool {
        matches!(action, Action::Show { .. })
    }

    fn secs(n: u64) -> Duration {
        Duration::from_secs(n)
    }

    // --- the delay -------------------------------------------------------

    #[test]
    fn quick_answer_under_the_delay_never_shows() {
        // The bug this whole delay exists for: a three-second answer used to
        // pop a game, which made the app unusable.
        let mut t = tracker();
        let t0 = Instant::now();
        assert_eq!(t.on_show_event("a", t0), Action::Nothing);
        assert_eq!(t.tick(t0 + secs(1)), Action::Nothing);
        assert_eq!(t.on_stop_event("a", t0 + secs(3)), Action::Nothing);
        assert_eq!(t.tick(t0 + secs(30)), Action::Nothing);
        assert!(!t.is_visible(), "a 3s answer opened a window");
    }

    #[test]
    fn sustained_thinking_shows_after_the_configured_delay() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert_eq!(t.tick(t0 + secs(9)), Action::Nothing, "shown too early");
        assert!(show(&t.tick(t0 + DELAY)));
        assert!(t.is_visible());
    }

    #[test]
    fn stopping_during_the_delay_window_cancels_the_pending_show() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        t.on_stop_event("a", t0 + secs(4));
        assert_eq!(t.tick(t0 + secs(600)), Action::Nothing);
    }

    #[test]
    fn changing_the_delay_setting_applies_to_the_next_bout() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.set_show_delay(secs(30));
        t.on_show_event("a", t0);
        assert_eq!(t.tick(t0 + secs(15)), Action::Nothing);
        assert!(show(&t.tick(t0 + secs(31))));
    }

    #[test]
    fn multiple_concurrent_sessions_still_show_exactly_once() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        t.on_show_event("b", t0 + secs(1));
        let mut shows = 0;
        for i in 0..40 {
            if show(&t.tick(t0 + secs(i))) {
                shows += 1;
            }
        }
        assert_eq!(shows, 1, "two sessions opened {shows} games");
    }

    #[test]
    fn flicker_burst_yields_exactly_one_game() {
        // Claude Code's terminal flicker fires UserPromptSubmit several times in
        // a few hundred ms; none of them may move the start of the bout.
        let mut t = tracker();
        let t0 = Instant::now();
        for i in 0..6 {
            assert_eq!(
                t.on_show_event("a", t0 + Duration::from_millis(i * 100)),
                Action::Nothing
            );
        }
        let mut shows = 0;
        for i in 0..30 {
            if show(&t.tick(t0 + secs(i))) {
                shows += 1;
            }
        }
        assert_eq!(shows, 1, "flicker burst opened {shows} games");
    }

    #[test]
    fn dismissing_early_does_not_reopen_while_the_same_bout_runs() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        t.on_hidden(); // user hit Esc straight away
        assert_eq!(
            t.tick(t0 + DELAY + secs(1)),
            Action::Nothing,
            "Esc must hold until the next prompt"
        );
        assert_eq!(t.tick(t0 + secs(120)), Action::Nothing);
    }

    #[test]
    fn a_later_prompt_shows_again_once_hidden() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        assert_eq!(t.on_stop_event("a", t0 + DELAY + secs(1)), Action::Freeze);
        t.on_hidden();

        t.on_show_event("a", t0 + secs(60));
        assert!(show(&t.tick(t0 + secs(60) + DELAY)));
    }

    #[test]
    fn a_show_while_playing_does_not_restart_the_run() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        assert_eq!(t.on_show_event("a", t0 + secs(30)), Action::Nothing);
        assert_eq!(t.tick(t0 + secs(60)), Action::Nothing);
    }

    // --- session bookkeeping ---------------------------------------------

    #[test]
    fn one_sessions_stop_does_not_close_anothers_game() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        // Session b starts thinking while the game from a is on screen.
        assert_eq!(t.on_show_event("b", t0 + DELAY + secs(1)), Action::Nothing);

        assert_eq!(
            t.on_stop_event("a", t0 + DELAY + secs(2)),
            Action::Nothing,
            "session b is still thinking — the window must stay"
        );
        assert_eq!(t.on_stop_event("b", t0 + DELAY + secs(3)), Action::Freeze);
    }

    #[test]
    fn stop_from_an_unknown_session_does_not_hide() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        assert_eq!(t.on_stop_event("ghost", t0 + DELAY + secs(1)), Action::Nothing);
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
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        // Terminal killed: no Stop ever arrives.
        assert_eq!(t.tick(t0 + secs(60)), Action::Nothing);
        assert_eq!(
            t.tick(t0 + SESSION_STALE_AFTER + secs(1)),
            Action::Freeze
        );
    }

    #[test]
    fn tick_is_idle_when_nothing_is_active() {
        let mut t = tracker();
        assert_eq!(t.tick(Instant::now()), Action::Nothing);
    }

    // --- pause ------------------------------------------------------------

    #[test]
    fn pause_suppresses_the_delayed_show() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.pause_for(Some(secs(900)), t0);
        t.on_show_event("a", t0);
        assert_eq!(t.tick(t0 + secs(60)), Action::Nothing);
        assert!(!t.is_visible());
    }

    #[test]
    fn pausing_blocks_shows_but_stop_still_hides() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
        assert_eq!(t.pause_for(Some(secs(900)), t0 + DELAY), Action::HideNow);
        t.on_hidden();

        t.on_show_event("a", t0 + DELAY + secs(1));
        assert_eq!(t.tick(t0 + DELAY + secs(60)), Action::Nothing);
        assert!(t.is_paused(t0 + secs(900)));
        assert!(!t.is_paused(t0 + secs(1000)));

        // Once the pause lapses a fresh bout shows again.
        let later = t0 + secs(1000);
        t.on_show_event("a", later);
        assert!(show(&t.tick(later + DELAY)));
    }

    #[test]
    fn indefinite_pause_never_expires_until_resumed() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.pause_for(None, t0);
        assert!(t.is_paused(t0 + secs(86_400)));
        t.resume();
        assert!(!t.is_paused(t0));
        t.on_show_event("a", t0);
        assert!(show(&t.tick(t0 + DELAY)));
    }

    #[test]
    fn a_restored_pause_is_honoured() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.restore_pause(Some(Pause::Until(t0 + secs(300))));
        t.on_show_event("a", t0);
        assert_eq!(t.tick(t0 + secs(60)), Action::Nothing);
        assert!(t.is_paused(t0 + secs(299)));
        assert!(!t.is_paused(t0 + secs(301)));
    }

    #[test]
    fn force_show_ignores_pause_delay_and_debounce() {
        let mut t = tracker();
        let t0 = Instant::now();
        t.pause_for(None, t0);
        assert!(show(&t.force_show(t0)));
        t.on_hidden();
        assert!(show(&t.force_show(t0 + Duration::from_millis(10))));
    }

    // --- game selection ---------------------------------------------------

    #[test]
    fn every_prompt_picks_a_registered_game() {
        let mut t = tracker();
        let t0 = Instant::now();
        let registered = games();
        for i in 0..80 {
            let start = t0 + secs(i * 60);
            t.on_show_event("a", start);
            let action = t.tick(start + DELAY);
            let Action::Show { game_id } = action else {
                panic!("expected a game at iteration {i}, got {action:?}");
            };
            assert!(registered.contains(&game_id), "unregistered game {game_id}");
            t.on_stop_event("a", start + DELAY + secs(1));
            t.on_hidden();
        }
    }

    #[test]
    fn consecutive_games_differ() {
        let mut t = tracker();
        let mut previous: Option<String> = None;
        let t0 = Instant::now();
        for i in 0..25 {
            let start = t0 + secs(i * 60);
            t.on_show_event("a", start);
            let action = t.tick(start + DELAY);
            let Action::Show { game_id } = action else {
                panic!("expected a game at iteration {i}, got {action:?}");
            };
            assert_ne!(Some(&game_id), previous.as_ref());
            previous = Some(game_id);
            t.on_stop_event("a", start + DELAY + secs(1));
            t.on_hidden();
        }
    }

    #[test]
    fn a_disabled_rotation_still_picks_from_what_is_left() {
        let mut t = Tracker::new(vec!["snake".into()], DELAY, 7);
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert_eq!(
            t.tick(t0 + DELAY),
            Action::Show {
                game_id: "snake".into()
            }
        );
    }

    #[test]
    fn an_empty_registry_shows_nothing() {
        let mut t = Tracker::new(Vec::new(), DELAY, 7);
        let t0 = Instant::now();
        t.on_show_event("a", t0);
        assert_eq!(t.tick(t0 + DELAY), Action::Nothing);
    }
}
