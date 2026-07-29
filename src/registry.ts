import type { GameFactory } from "./game";
import { FREE_REGISTRY } from "./games/free";
import { PRO_REGISTRY } from "./games/pro";

/**
 * Every game module the app knows about. Rust picks an id from tiers.rs —
 * keep game ids in sync when adding a game.
 */
export const REGISTRY: Record<string, GameFactory> = {
  ...FREE_REGISTRY,
  ...PRO_REGISTRY,
};

export const GAME_IDS = Object.keys(REGISTRY);

export { FREE_GAME_IDS, PRO_GAME_IDS, isProGame, pickableGameIds } from "./tiers";
export { FREE_REGISTRY, PRO_REGISTRY };
