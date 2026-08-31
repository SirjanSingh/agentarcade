//! The registered game ids.
//!
//! Kept in sync with `src/registry.ts` by `scripts/check-game-ids.mjs`, which
//! runs in CI — adding a game to one side and not the other fails the build.
//!
//! There is no free/pro split: every game ships to everyone.

pub const GAME_IDS: &[&str] = &[
    "snake",
    "typing",
    "aim",
    "reaction",
    "gradient",
    "metronome",
    "untangle",
    "boulder",
];

/// The pool the picker chooses from.
pub fn game_ids() -> Vec<String> {
    GAME_IDS.iter().map(|s| s.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_game_is_registered() {
        assert_eq!(game_ids().len(), 8);
    }

    #[test]
    fn ids_are_unique() {
        let mut seen = std::collections::HashSet::new();
        for id in GAME_IDS {
            assert!(seen.insert(id), "duplicate game id: {id}");
        }
    }
}
