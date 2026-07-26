/**
 * Thin wrapper over the Tauri IPC, with a working fallback when the page is
 * opened in a plain browser (`npm run dev`).
 *
 * The fallback is not decoration: it is how the games get exercised without a
 * Rust toolchain. In browser mode, stats live in localStorage and
 * `window.claudegame` drives the same events Rust would emit.
 */

import type { GameStats } from "./types";

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

function readLocalStats(): GameStats {
  try {
    const raw = localStorage.getItem(LOCAL_STATS_KEY);
    if (raw) return JSON.parse(raw) as GameStats;
  } catch {
    // Corrupt local stats are not worth a crash — start fresh.
  }
  return { version: 1, games: {}, totals: { sessions: 0, secondsPlayed: 0 } };
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
  localStorage.setItem(LOCAL_STATS_KEY, JSON.stringify(stats));
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

/**
 * Browser-mode driver. Lets a human (or a Playwright script) exercise the exact
 * event sequence Rust produces: claudegame.show("snake") then claudegame.stop().
 */
export function installBrowserDriver(gameIds: readonly string[]): void {
  if (isTauri) return;
  const driver = {
    games: gameIds,
    show: (gameId?: string) =>
      emitLocal("game:show", { gameId: gameId ?? gameIds[Math.floor(Math.random() * gameIds.length)] }),
    stop: () => emitLocal("game:freeze", null),
    hide: () => emitLocal("game:hide", null),
  };
  (window as unknown as { claudegame: typeof driver }).claudegame = driver;
  // eslint-disable-next-line no-console
  console.info("[claudegame] browser mode — try claudegame.show('snake') then claudegame.stop()");
}

export { isTauri };
