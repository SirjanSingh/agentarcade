import { FREE_GAME_IDS } from "./games/free";
import { PRO_GAME_IDS } from "./games/pro";

/**
 * Free vs Pro game tiers. Keep in sync with src-tauri/src/logic/tiers.rs.
 *
 * Free: src/games/free/
 * Pro:  src/games/pro/  (gated until license; future private repo)
 */

export { FREE_GAME_IDS, PRO_GAME_IDS };

export const ALL_GAME_IDS = [...FREE_GAME_IDS, ...PRO_GAME_IDS] as const;

export type FreeGameId = (typeof FREE_GAME_IDS)[number];
export type ProGameId = (typeof PRO_GAME_IDS)[number];
export type GameId = FreeGameId | ProGameId;

export function isProGame(id: string): boolean {
  return (PRO_GAME_IDS as readonly string[]).includes(id);
}

/** Games the random picker may choose for this install. */
export function pickableGameIds(isPro: boolean): readonly string[] {
  return isPro ? ALL_GAME_IDS : FREE_GAME_IDS;
}
