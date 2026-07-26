//! Local, file-backed stats. No accounts, no network.
//!
//! Shape mirrors src/types.ts exactly (camelCase) so the frontend can consume
//! the same JSON the file holds.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

const VERSION: u32 = 1;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct GameStatEntry {
    pub plays: u64,
    pub best: Option<f64>,
    pub total: f64,
    pub last_played: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Totals {
    pub sessions: u64,
    pub seconds_played: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub version: u32,
    pub games: BTreeMap<String, GameStatEntry>,
    pub totals: Totals,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            version: VERSION,
            games: BTreeMap::new(),
            totals: Totals::default(),
        }
    }
}

impl Stats {
    /// Read stats from disk. A corrupt file is preserved as `.bak` rather than
    /// silently overwritten — the user may want to see what happened.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Stats>(&raw) {
            Ok(stats) => stats,
            Err(err) => {
                eprintln!("[claudegame] stats.json unreadable ({err}); starting fresh");
                let _ = fs::write(path.with_extension("json.bak"), raw);
                Self::default()
            }
        }
    }

    pub fn record(&mut self, game_id: &str, score: f64, duration_ms: f64, lower_is_better: bool) {
        let entry = self.games.entry(game_id.to_string()).or_default();
        entry.plays += 1;
        entry.total += score;
        entry.best = Some(match entry.best {
            Some(best) if lower_is_better => best.min(score),
            Some(best) => best.max(score),
            None => score,
        });
        entry.last_played = Some(now_iso8601());

        self.totals.sessions += 1;
        self.totals.seconds_played += (duration_ms / 1000.0).round().max(0.0) as u64;
    }

    /// Write via a temp file + rename so a crash mid-write cannot truncate the
    /// existing stats. `fs::rename` replaces the destination on all three
    /// platforms.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let tmp: PathBuf = path.with_extension("json.tmp");
        fs::write(&tmp, serde_json::to_vec_pretty(self)?)?;
        fs::rename(&tmp, path)
    }
}

fn now_iso8601() -> String {
    chrono::Utc::now().format("%Y-%m-%dT%H:%M:%S%.3fZ").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_is_better_by_default() {
        let mut stats = Stats::default();
        stats.record("snake", 5.0, 1000.0, false);
        stats.record("snake", 12.0, 1000.0, false);
        stats.record("snake", 3.0, 1000.0, false);
        let entry = &stats.games["snake"];
        assert_eq!(entry.plays, 3);
        assert_eq!(entry.best, Some(12.0));
        assert_eq!(entry.total, 20.0);
    }

    #[test]
    fn reaction_times_keep_the_lowest() {
        let mut stats = Stats::default();
        stats.record("reaction", 320.0, 4000.0, true);
        stats.record("reaction", 210.0, 4000.0, true);
        stats.record("reaction", 480.0, 4000.0, true);
        assert_eq!(stats.games["reaction"].best, Some(210.0));
    }

    #[test]
    fn totals_accumulate_across_games() {
        let mut stats = Stats::default();
        stats.record("snake", 5.0, 12_400.0, false);
        stats.record("typing", 60.0, 30_000.0, false);
        assert_eq!(stats.totals.sessions, 2);
        assert_eq!(stats.totals.seconds_played, 42);
    }

    #[test]
    fn save_then_load_round_trips() {
        let dir = std::env::temp_dir().join(format!("claudegame-test-{}", std::process::id()));
        let path = dir.join("stats.json");
        let _ = fs::create_dir_all(&dir);

        let mut stats = Stats::default();
        stats.record("snake", 9.0, 5000.0, false);
        stats.save(&path).unwrap();

        let loaded = Stats::load(&path);
        assert_eq!(loaded.games["snake"].best, Some(9.0));
        assert_eq!(loaded.totals.sessions, 1);
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn missing_file_yields_empty_stats() {
        let stats = Stats::load(Path::new("definitely/not/here/stats.json"));
        assert_eq!(stats.totals.sessions, 0);
        assert!(stats.games.is_empty());
    }

    #[test]
    fn corrupt_file_is_backed_up_not_lost() {
        let dir = std::env::temp_dir().join(format!("claudegame-corrupt-{}", std::process::id()));
        let path = dir.join("stats.json");
        let _ = fs::create_dir_all(&dir);
        fs::write(&path, "{ not json at all").unwrap();

        let stats = Stats::load(&path);
        assert!(stats.games.is_empty(), "should start fresh");
        let backup = path.with_extension("json.bak");
        assert!(backup.exists(), "the unreadable file must be kept");
        assert_eq!(fs::read_to_string(backup).unwrap(), "{ not json at all");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn serialised_keys_match_the_typescript_types() {
        let mut stats = Stats::default();
        stats.record("snake", 1.0, 1000.0, false);
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"lastPlayed\""), "{json}");
        assert!(json.contains("\"secondsPlayed\""), "{json}");
    }
}
