/**
 * Thin wrapper over the Tauri IPC, with a working fallback when the page is
 * opened in a plain browser (`npm run dev`).
 *
 * The fallback is not decoration: it is how the games get exercised without a
 * Rust toolchain. In browser mode, stats live in localStorage and
 * `window.claudegame` drives the same events Rust would emit.
 */

import { STATS_VERSION, type GameStats } from "./types";

/** Keep in step with RECENT_LIMIT in src-tauri/src/stats.rs. */
const RECENT_LIMIT = 100;

const isTauri = typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

type Handler<T> = (payload: T) => void;

const browserHandlers = new Map<string, Set<Handler<unknown>>>();

export async function listen<T>(event: string, handler: Handler<T>): Promise<void> {
  if (isTauri) {
    const { listen: tauriListen } = await import("@tauri-apps/api/event");
    await tauriListen<T>(event, (e) => handler(e.payload));
    return;
  }
  let set = browserHandlers.get(event);
  if (!set) {
    set = new Set();
    browserHandlers.set(event, set);
  }
  set.add(handler as Handler<unknown>);
}

function emitLocal(event: string, payload: unknown): void {
  for (const handler of browserHandlers.get(event) ?? []) handler(payload);
}

async function invoke<T>(cmd: string, args?: Record<string, unknown>): Promise<T> {
  const { invoke: tauriInvoke } = await import("@tauri-apps/api/core");
  return tauriInvoke<T>(cmd, args);
}

const LOCAL_STATS_KEY = "claudegame.stats";

function emptyStats(): GameStats {
  return {
    version: STATS_VERSION,
    deviceId: crypto.randomUUID(),
    games: {},
    recent: [],
    totals: { sessions: 0, secondsPlayed: 0 },
  };
}

function readLocalStats(): GameStats {
  try {
    const raw = localStorage.getItem(LOCAL_STATS_KEY);
    if (raw) {
      // Fill in anything a v1 file predates rather than discarding its bests.
      const parsed = JSON.parse(raw) as Partial<GameStats>;
      return {
        version: STATS_VERSION,
        deviceId: parsed.deviceId ?? crypto.randomUUID(),
        games: parsed.games ?? {},
        recent: parsed.recent ?? [],
        totals: parsed.totals ?? { sessions: 0, secondsPlayed: 0 },
      };
    }
  } catch {
    // Corrupt local stats are not worth a crash — start fresh.
  }
  return emptyStats();
}

export async function getStats(): Promise<GameStats> {
  if (isTauri) return invoke<GameStats>("get_stats");
  return readLocalStats();
}

export async function recordResult(
  gameId: string,
  score: number,
  durationMs: number,
  lowerIsBetter: boolean,
): Promise<void> {
  if (isTauri) {
    await invoke("record_result", { gameId, score, durationMs, lowerIsBetter });
    return;
  }
  const stats = readLocalStats();
  const entry = stats.games[gameId] ?? { plays: 0, best: null, total: 0, lastPlayed: null };
  entry.plays += 1;
  entry.total += score;
  if (entry.best === null || (lowerIsBetter ? score < entry.best : score > entry.best)) {
    entry.best = score;
  }
  entry.lastPlayed = new Date().toISOString();
  stats.games[gameId] = entry;
  stats.totals.sessions += 1;
  stats.totals.secondsPlayed += Math.round(durationMs / 1000);
  stats.recent.unshift({
    id: crypto.randomUUID(),
    gameId,
    score,
    durationMs: Math.round(durationMs),
    at: entry.lastPlayed,
  });
  stats.recent = stats.recent.slice(0, RECENT_LIMIT);
  localStorage.setItem(LOCAL_STATS_KEY, JSON.stringify(stats));
  window.dispatchEvent(new CustomEvent("claudegame:recorded"));
}

/**
 * A game the app already decided to show before this webview finished loading.
 * Tauri does not replay events, so without this a hook landing during startup
 * would open an empty window.
 */
export async function pendingGame(): Promise<string | null> {
  if (isTauri) return invoke<string | null>("pending_game");
  return null;
}

/** Ask Rust to hide the window and hand focus back to the terminal. */
export async function requestHide(): Promise<void> {
  if (isTauri) {
    await invoke("hide_window");
    return;
  }
  emitLocal("game:hide", null);
}

/** Stands in for Rust when the page runs outside Tauri. */
export interface BrowserDriver {
  readonly games: readonly string[];
  /** What UserPromptSubmit does. */
  show(gameId?: string): void;
  /** What Stop does. */
  stop(): void;
  hide(): void;
}

/**
 * Browser-mode driver. Lets a human (or a Playwright script) exercise the exact
 * event sequence Rust produces: claudegame.show("snake") then claudegame.stop().
 */
export function installBrowserDriver(gameIds: readonly string[]): BrowserDriver | null {
  if (isTauri) return null;
  const driver: BrowserDriver = {
    games: gameIds,
    show: (gameId?: string) =>
      emitLocal("game:show", { gameId: gameId ?? gameIds[Math.floor(Math.random() * gameIds.length)] }),
    stop: () => emitLocal("game:freeze", null),
    hide: () => emitLocal("game:hide", null),
  };
  (window as unknown as { claudegame: BrowserDriver }).claudegame = driver;
  return driver;
}

export { isTauri };
