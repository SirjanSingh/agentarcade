/** Mirrors the serde shape written to stats.json by src-tauri/src/stats.rs. */

export interface GameStatEntry {
  plays: number;
  best: number | null;
  total: number;
  lastPlayed: string | null;
}

export interface GameStats {
  version: number;
  games: Record<string, GameStatEntry>;
  totals: {
    sessions: number;
    secondsPlayed: number;
  };
}

/** Payload of the `game:show` event emitted by Rust. */
export interface ShowPayload {
  gameId: string;
}
