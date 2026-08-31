import type { GameFactory } from "./game";
import { createAim } from "./games/aim";
import { createBoulder } from "./games/boulder";
import { createGradient } from "./games/gradient";
import { createMetronome } from "./games/metronome";
import { createReaction } from "./games/reaction";
import { createSnake } from "./games/snake";
import { createTyping } from "./games/typing";
import { createUntangle } from "./games/untangle";

/**
 * Every game the app knows about. There is no free/paid split: all of these
 * ship to everyone.
 *
 * Adding a game is one new file in src/games/ implementing GameModule, one line
 * here, and one id in src-tauri/src/logic/games.rs. CI fails if those last two
 * drift apart — see scripts/check-game-ids.mjs.
 */
export const REGISTRY: Record<string, GameFactory> = {
  snake: createSnake,
  typing: createTyping,
  aim: createAim,
  reaction: createReaction,
  gradient: createGradient,
  metronome: createMetronome,
  untangle: createUntangle,
  boulder: createBoulder,
};

export const GAME_IDS: readonly string[] = Object.keys(REGISTRY);
