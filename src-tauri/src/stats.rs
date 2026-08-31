//! Local, file-backed stats. No accounts, no network.
//!
//! Shape mirrors src/types.ts exactly (camelCase) so the frontend can consume
//! the same JSON the file holds.

use std::collections::BTreeMap;
use std::fs;
use std::path::Path;

use serde::{Deserialize, Serialize};

/// v1 had no device id and no run history. v2 adds both so a sync layer can be
/// bolted on later without changing how anything here works.
const VERSION: u32 = 2;
/// Keep in step with RECENT_LIMIT in src/bridge.ts.
const RECENT_LIMIT: usize = 100;

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

/// One completed run, with a stable id so a future upload can be idempotent.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub game_id: String,
    pub score: f64,
    pub duration_ms: f64,
    pub at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Stats {
    pub version: u32,
    /// Random per-install id. Not derived from anything about the machine or
    /// the user — it only exists to tell two installs apart if syncing is added.
    #[serde(default = "new_device_id")]
    pub device_id: String,
    pub games: BTreeMap<String, GameStatEntry>,
    /// Newest first, capped at RECENT_LIMIT.
    #[serde(default)]
    pub recent: Vec<RunRecord>,
    pub totals: Totals,
}

impl Default for Stats {
    fn default() -> Self {
        Self {
            version: VERSION,
            device_id: new_device_id(),
            games: BTreeMap::new(),
            recent: Vec::new(),
            totals: Totals::default(),
        }
    }
}

/// An opaque unique id, for the device and for each run. Not worth a uuid
/// dependency for identifiers that only have to not collide.
///
/// The counter is what makes it safe: the clock alone has coarse enough
/// granularity on Windows that two ids minted in the same tick would match.
fn new_id() -> String {
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::time::{SystemTime, UNIX_EPOCH};

    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let seq = COUNTER.fetch_add(1, Ordering::Relaxed) as u128;
    let nanos = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let addr = &nanos as *const u128 as usize as u128;
    let mix = nanos ^ addr.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    format!("{:024x}{:08x}", mix & 0xffff_ffff_ffff_ffff_ffff_ffff, seq as u64)
}

fn new_device_id() -> String {
    new_id()
}

impl Stats {
    /// Read stats from disk. A corrupt file is preserved as `.bak` rather than
    /// silently overwritten — the user may want to see what happened.
    pub fn load(path: &Path) -> Self {
        let Ok(raw) = fs::read_to_string(path) else {
            return Self::default();
        };
        match serde_json::from_str::<Stats>(&raw) {
            // serde defaults fill in device_id and recent for a v1 file, so an
            // upgrade keeps every existing personal best.
            Ok(mut stats) => {
                stats.version = VERSION;
                stats
            }
            Err(err) => {
                eprintln!("[agentarcade] stats.json unreadable ({err}); starting fresh");
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
        let at = now_iso8601();
        entry.last_played = Some(at.clone());

        self.totals.sessions += 1;
        self.totals.seconds_played += (duration_ms / 1000.0).round().max(0.0) as u64;

        self.recent.insert(
            0,
            RunRecord {
                id: new_id(),
                game_id: game_id.to_string(),
                score,
                duration_ms,
                at,
            },
        );
        self.recent.truncate(RECENT_LIMIT);
    }

    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        crate::atomic_write::write(path, &serde_json::to_vec_pretty(self)?)
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
        let dir = std::env::temp_dir().join(format!("agentarcade-test-{}", std::process::id()));
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
        let dir = std::env::temp_dir().join(format!("agentarcade-corrupt-{}", std::process::id()));
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
    fn recent_runs_are_newest_first_and_capped() {
        let mut stats = Stats::default();
        for i in 0..(RECENT_LIMIT + 25) {
            stats.record("snake", i as f64, 1000.0, false);
        }
        assert_eq!(stats.recent.len(), RECENT_LIMIT);
        assert_eq!(stats.recent[0].score, (RECENT_LIMIT + 24) as f64);
        assert_eq!(stats.recent[0].game_id, "snake");
        assert!(stats.recent.windows(2).all(|w| w[0].score > w[1].score));
    }

    #[test]
    fn run_ids_are_unique() {
        let mut stats = Stats::default();
        for _ in 0..50 {
            stats.record("snake", 1.0, 1000.0, false);
        }
        let ids: std::collections::HashSet<&String> = stats.recent.iter().map(|r| &r.id).collect();
        assert_eq!(ids.len(), stats.recent.len(), "duplicate run ids would break sync later");
    }

    #[test]
    fn a_v1_file_upgrades_without_losing_bests() {
        let dir = std::env::temp_dir().join(format!("agentarcade-v1-{}", std::process::id()));
        let path = dir.join("stats.json");
        let _ = fs::create_dir_all(&dir);
        // Exactly what v1 wrote: no deviceId, no recent.
        fs::write(
            &path,
            r#"{"version":1,"games":{"snake":{"plays":7,"best":19.0,"total":54.0,"lastPlayed":"2026-07-01T00:00:00.000Z"}},"totals":{"sessions":7,"secondsPlayed":300}}"#,
        )
        .unwrap();

        let stats = Stats::load(&path);
        assert_eq!(stats.version, VERSION);
        assert_eq!(stats.games["snake"].best, Some(19.0));
        assert_eq!(stats.totals.sessions, 7);
        assert!(stats.recent.is_empty());
        assert!(!stats.device_id.is_empty(), "upgrade must mint a device id");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn device_id_survives_a_round_trip() {
        let dir = std::env::temp_dir().join(format!("agentarcade-dev-{}", std::process::id()));
        let path = dir.join("stats.json");
        let _ = fs::create_dir_all(&dir);

        let stats = Stats::default();
        let id = stats.device_id.clone();
        stats.save(&path).unwrap();
        assert_eq!(Stats::load(&path).device_id, id, "a new id each launch would break sync");
        let _ = fs::remove_dir_all(&dir);
    }

    #[test]
    fn serialised_keys_match_the_typescript_types() {
        let mut stats = Stats::default();
        stats.record("snake", 1.0, 1000.0, false);
        let json = serde_json::to_string(&stats).unwrap();
        assert!(json.contains("\"lastPlayed\""), "{json}");
        assert!(json.contains("\"secondsPlayed\""), "{json}");
        assert!(json.contains("\"deviceId\""), "{json}");
        assert!(json.contains("\"gameId\""), "{json}");
        assert!(json.contains("\"durationMs\""), "{json}");
    }
}
