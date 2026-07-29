/** Mirrors the serde shape written to stats.json by src-tauri/src/stats.rs. */

/** Current on-disk schema. v1 had no deviceId and no run history. */
export const STATS_VERSION = 2;

export interface GameStatEntry {
  plays: number;
  best: number | null;
  total: number;
  lastPlayed: string | null;
}

/**
 * One completed run. Carries a stable id and a timestamp so a future sync
 * layer can upload runs idempotently and merge across devices, without
 * anything about the local format having to change.
 */
export interface RunRecord {
  id: string;
  gameId: string;
  score: number;
  durationMs: number;
  at: string;
}

export interface GameStats {
  version: number;
  /**
   * Random per-install id. Not tied to a person or machine — it exists so runs
   * from two installs can be told apart if syncing is ever added.
   */
  deviceId: string;
  games: Record<string, GameStatEntry>;
  /** Newest first, capped — enough for sparklines and a recent-runs list. */
  recent: RunRecord[];
  totals: {
    sessions: number;
    secondsPlayed: number;
  };
  /** Unlocks Pro games once a license is activated. */
  proUnlocked?: boolean;
}

/** Payload of the `game:show` event emitted by Rust. */
export interface ShowPayload {
  gameId: string;
}
