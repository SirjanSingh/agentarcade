import type { GameFactory } from "./game";
import { createSnake } from "./games/snake";
import { createReaction } from "./games/reaction";
import { createTyping } from "./games/typing";

/**
 * The one place games are registered. Rust picks an id from this same list
 * (src-tauri/src/core/picker.rs) — keep the two in sync when adding a game.
 */
export const REGISTRY: Record<string, GameFactory> = {
  snake: createSnake,
  reaction: createReaction,
  typing: createTyping,
};

export const GAME_IDS = Object.keys(REGISTRY);
