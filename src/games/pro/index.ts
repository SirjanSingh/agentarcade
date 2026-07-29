import type { GameFactory } from "../../game";
import { createBoulder } from "./boulder";
import { createGradient } from "./gradient";
import { createMetronome } from "./metronome";
import { createUntangle } from "./untangle";

/**
 * Pro-tier games — gated at runtime until a license is activated.
 * A separate private repo will ship Pro builds with extra features later.
 */
export const PRO_REGISTRY: Record<string, GameFactory> = {
  gradient: createGradient,
  metronome: createMetronome,
  untangle: createUntangle,
  boulder: createBoulder,
};

export const PRO_GAME_IDS = Object.keys(PRO_REGISTRY);
