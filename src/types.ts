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

/**
 * One generated game, as returned by the `list_packs` command. Mirrors the
 * serde shape of `Pack` in src-tauri/src/packs.rs.
 */
export interface Pack {
  id: string;
  label: string;
  unit: string;
  lowerIsBetter: boolean;
  /** What the user asked their agent for. Shown in settings. */
  prompt: string;
  createdAt: string;
  /**
   * Where the iframe loads the game from. Built in Rust because the scheme is
   * spelled differently on Windows than elsewhere.
   */
  url: string;
}

/** Payload of the `game:show` event emitted by Rust. */
export interface ShowPayload {
  gameId: string;
}
