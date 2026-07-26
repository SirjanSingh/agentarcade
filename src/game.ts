/**
 * The contract every mini-game implements.
 *
 * Adding a game means writing one module that satisfies this interface and
 * adding one line to REGISTRY in registry.ts. Nothing else in the app needs
 * to know the game exists.
 */

export interface GameApi {
  /**
   * Report the run's current score. Safe to call repeatedly — the shell keeps
   * the latest value and records it when the run ends. Games that only produce
   * a score at the end (reaction) call it once.
   */
  reportScore(score: number): void;
  /** Personal best for this game, or null if never played. */
  readonly best: number | null;
}

export interface GameModule {
  readonly id: string;
  readonly label: string;
  /** e.g. "pts", "ms", "wpm" — rendered next to the score. */
  readonly unit: string;
  /** True for games where a smaller score is better (reaction time). */
  readonly lowerIsBetter: boolean;
  /** Render into `root`. Called once per run. */
  mount(root: HTMLElement, api: GameApi): void;
  /**
   * Stop the run but leave the final frame on screen. Called when Claude
   * finishes, just before the score card. Must be idempotent.
   */
  freeze(): void;
  /** Release timers, rAF handles and listeners. Must be idempotent. */
  destroy(): void;
}

export type GameFactory = () => GameModule;

/** Format a score for display, e.g. `312 ms`. */
export function formatScore(game: GameModule, score: number | null): string {
  if (score === null) return "—";
  return `${Math.round(score)} ${game.unit}`;
}

/** True if `score` beats `best` under this game's ordering. */
export function isBetter(game: GameModule, score: number, best: number | null): boolean {
  if (best === null) return true;
  return game.lowerIsBetter ? score < best : score > best;
}
