import type { GameFactory } from "../../game";
import { createAim } from "./aim";
import { createReaction } from "./reaction";
import { createSnake } from "./snake";
import { createTyping } from "./typing";

/** Free-tier games — always included in the public repo. */
export const FREE_REGISTRY: Record<string, GameFactory> = {
  snake: createSnake,
  reaction: createReaction,
  typing: createTyping,
  aim: createAim,
};

export const FREE_GAME_IDS = Object.keys(FREE_REGISTRY);
