//! Game selection: uniformly random, never the same game twice in a row.
//!
//! The randomness is a tiny splitmix64 rather than the `rand` crate — it keeps
//! the dependency list short and, more usefully, makes every test here fully
//! deterministic by passing the roll in directly.

/// Pick a game, excluding `last` unless that would leave nothing to pick.
///
/// `roll` is any random u64; only its remainder matters.
pub fn choose_excluding<'a>(games: &'a [String], last: Option<&str>, roll: u64) -> Option<&'a str> {
    if games.is_empty() {
        return None;
    }
    let candidates: Vec<&String> = games
        .iter()
        .filter(|g| Some(g.as_str()) != last)
        .collect();
    // A single registered game must still be playable, so fall back to the
    // full list rather than returning None.
    let pool = if candidates.is_empty() {
        games.iter().collect::<Vec<&String>>()
    } else {
        candidates
    };
    let index = (roll % pool.len() as u64) as usize;
    Some(pool[index].as_str())
}

/// splitmix64 — small, fast, and good enough for choosing between three games.
#[derive(Debug, Clone)]
pub struct Rng {
    state: u64,
}

impl Rng {
    pub fn new(seed: u64) -> Self {
        // A zero seed would emit a fixed sequence; nudge it off zero.
        Self {
            state: if seed == 0 { 0x9E37_79B9_7F4A_7C15 } else { seed },
        }
    }

    pub fn next_u64(&mut self) -> u64 {
        self.state = self.state.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.state;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn games() -> Vec<String> {
        vec!["snake".into(), "reaction".into(), "typing".into()]
    }

    #[test]
    fn never_repeats_the_previous_game() {
        let games = games();
        let mut rng = Rng::new(42);
        let mut last: Option<String> = None;
        for _ in 0..200 {
            let picked = choose_excluding(&games, last.as_deref(), rng.next_u64())
                .expect("registry is not empty")
                .to_string();
            assert_ne!(Some(&picked), last.as_ref(), "picked the same game twice running");
            last = Some(picked);
        }
    }

    #[test]
    fn eventually_picks_every_game() {
        let games = games();
        let mut rng = Rng::new(7);
        let mut last: Option<String> = None;
        let mut seen = std::collections::HashSet::new();
        for _ in 0..200 {
            let picked = choose_excluding(&games, last.as_deref(), rng.next_u64())
                .unwrap()
                .to_string();
            seen.insert(picked.clone());
            last = Some(picked);
        }
        assert_eq!(seen.len(), 3, "some game is unreachable: {seen:?}");
    }

    #[test]
    fn single_game_registry_keeps_working() {
        let only = vec!["snake".to_string()];
        // Excluding the only game must not deadlock or return None.
        assert_eq!(choose_excluding(&only, Some("snake"), 0), Some("snake"));
        assert_eq!(choose_excluding(&only, Some("snake"), 999), Some("snake"));
    }

    #[test]
    fn empty_registry_returns_none() {
        assert_eq!(choose_excluding(&[], None, 3), None);
    }

    #[test]
    fn roll_selects_deterministically() {
        let games = games();
        assert_eq!(choose_excluding(&games, None, 0), Some("snake"));
        assert_eq!(choose_excluding(&games, None, 1), Some("reaction"));
        assert_eq!(choose_excluding(&games, None, 2), Some("typing"));
        // With "snake" excluded the pool is [reaction, typing].
        assert_eq!(choose_excluding(&games, Some("snake"), 0), Some("reaction"));
        assert_eq!(choose_excluding(&games, Some("snake"), 1), Some("typing"));
    }

    #[test]
    fn rng_is_not_stuck() {
        let mut rng = Rng::new(0);
        let a = rng.next_u64();
        let b = rng.next_u64();
        assert_ne!(a, b);
    }
}
