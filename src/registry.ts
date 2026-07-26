import type { GameFactory } from "./game";
import { createSnake } from "./games/snake";
import { createReaction } from "./games/reaction";
import { createTyping } from "./games/typing";
import { createGradient } from "./games/gradient";
import { createMetronome } from "./games/metronome";
import { createAim } from "./games/aim";
import { createUntangle } from "./games/untangle";

/**
 * The one place games are registered. Rust picks an id from this same list
 * (GAMES in src-tauri/src/main.rs) — keep the two in sync when adding a game.
 */
export const REGISTRY: Record<string, GameFactory> = {
  snake: createSnake,
  reaction: createReaction,
  typing: createTyping,
  gradient: createGradient,
  metronome: createMetronome,
  aim: createAim,
  untangle: createUntangle,
};

export const GAME_IDS = Object.keys(REGISTRY);
