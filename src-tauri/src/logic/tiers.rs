//! Free vs Pro game tiers. Keep in sync with src/tiers.ts.

pub const FREE_GAMES: &[&str] = &["snake", "typing", "aim", "reaction"];
pub const PRO_GAMES: &[&str] = &["gradient", "metronome", "untangle", "boulder"];

pub fn is_pro_game(id: &str) -> bool {
    PRO_GAMES.contains(&id)
}

pub fn pickable_games(pro_unlocked: bool) -> Vec<String> {
    let mut games: Vec<String> = FREE_GAMES.iter().map(|s| s.to_string()).collect();
    if pro_unlocked {
        games.extend(PRO_GAMES.iter().map(|s| s.to_string()));
    }
    games
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn free_pool_has_four_games() {
        assert_eq!(pickable_games(false).len(), 4);
    }

    #[test]
    fn pro_pool_has_all_eight() {
        assert_eq!(pickable_games(true).len(), 8);
    }
}
