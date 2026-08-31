import { formatScore, isBetter, type GameModule } from "./game";
import { installGameTabs } from "./game-tabs";
import { GAME_IDS, REGISTRY } from "./registry";
import { getStats, installBrowserDriver, listen, pendingGame, recordResult, requestHide } from "./bridge";
import { installDevBar } from "./devbar";
import { renderStats } from "./stats-view";
import type { ShowPayload } from "./types";

const SCORECARD_MS = 1500;

const stage = document.getElementById("stage") as HTMLElement;
const label = document.getElementById("game-label") as HTMLElement;
const bestEl = document.getElementById("best") as HTMLElement;
const waitingEl = document.getElementById("waiting") as HTMLElement;
const scorecard = document.getElementById("scorecard") as HTMLElement;
const finalScore = document.getElementById("final-score") as HTMLElement;
const finalSub = document.getElementById("final-sub") as HTMLElement;
const tabsMount = document.getElementById("game-tabs-mount") as HTMLElement;

let current: GameModule | null = null;
let currentGameId: string | null = null;
let currentScore: number | null = null;
let currentBest: number | null = null;
let startedAt = 0;
let hideTimer: number | undefined;
let showingStats = false;
/** Bumps on every show; stale async starts bail out when this moves. */
let startSeq = 0;
/** In-flight mount — finish/stop must wait so freeze() hits a live game. */
let starting: Promise<void> | null = null;

const gameTabs = installGameTabs(
  tabsMount,
  async () => GAME_IDS,
  (gameId) => void switchGame(gameId),
);

/** Save a partial score before leaving a game (tab switch or dismiss). */
async function recordPartialIfAny(): Promise<void> {
  if (current && currentScore !== null && Number.isFinite(currentScore)) {
    current.freeze();
    await recordResult(current.id, currentScore, performance.now() - startedAt, current.lowerIsBetter);
    currentScore = null;
  }
}

async function switchGame(gameId: string): Promise<void> {
  if (showingStats) {
    void startGame(gameId);
    return;
  }
  if (currentGameId === gameId && current) return;
  await recordPartialIfAny();
  void startGame(gameId);
}

async function cycleGame(direction: 1 | -1): Promise<void> {
  if (showingStats || !scorecard.hidden) return;
  const ids = GAME_IDS;
  if (ids.length === 0) return;
  const idx = Math.max(0, ids.indexOf(currentGameId ?? ids[0]));
  const next = ids[(idx + direction + ids.length) % ids.length];
  await switchGame(next);
}

async function showStats(): Promise<void> {
  if (starting) await starting;
  teardown();
  showingStats = true;
  gameTabs.setActive(null);
  tabsMount.querySelector("#game-tabs")?.setAttribute("hidden", "");
  label.textContent = "Stats";
  bestEl.textContent = "";
  waitingEl.hidden = true;
  renderStats(stage, await getStats());
}

async function startGame(gameId: string): Promise<void> {
  const seq = ++startSeq;
  if (starting) await starting;
  teardown();
  showingStats = false;

  const factory = REGISTRY[gameId] ?? REGISTRY[GAME_IDS[0]];
  const game = factory();

  const run = (async () => {
    const stats = await getStats();
    if (seq !== startSeq) return;

    currentScore = null;
    startedAt = performance.now();
    currentBest = stats.games[game.id]?.best ?? null;
    currentGameId = game.id;

    label.textContent = game.label;
    gameTabs.setActive(game.id);
    tabsMount.querySelector("#game-tabs")?.removeAttribute("hidden");
    bestEl.textContent = currentBest === null ? "no personal best yet" : `best ${formatScore(game, currentBest)}`;
    waitingEl.hidden = false;
    scorecard.hidden = true;

    current = game;
    game.mount(stage, {
      reportScore(score: number) {
        if (current === game) currentScore = score;
      },
      get best() {
        return currentBest;
      },
    });
  })();

  starting = run;
  await run;
  if (starting === run) starting = null;
}

/** The agent finished: stop the run, show the result, then get out of the way. */
async function finishAndHide(): Promise<void> {
  if (starting) await starting;
  // Nothing to score if the user was reading their stats when the agent finished.
  if (!current || showingStats) {
    await requestHide();
    return;
  }
  const game = current;
  game.freeze();
  waitingEl.hidden = true;

  if (currentScore !== null && Number.isFinite(currentScore)) {
    const beat = isBetter(game, currentScore, currentBest);
    finalScore.textContent = formatScore(game, currentScore);
    finalSub.textContent = beat
      ? currentBest === null
        ? "first run on the board"
        : `new best — beat ${formatScore(game, currentBest)}`
      : `best ${formatScore(game, currentBest)}`;
    await recordResult(game.id, currentScore, performance.now() - startedAt, game.lowerIsBetter);
  } else {
    finalScore.textContent = "—";
    finalSub.textContent = "no score this round";
  }
  scorecard.hidden = false;

  window.clearTimeout(hideTimer);
  hideTimer = window.setTimeout(() => void requestHide(), SCORECARD_MS);
}

function teardown(): void {
  window.clearTimeout(hideTimer);
  current?.destroy();
  current = null;
  currentGameId = null;
  currentScore = null;
  stage.replaceChildren();
  scorecard.hidden = true;
}

/** Esc dismisses immediately, but the run still counts. */
async function dismissNow(): Promise<void> {
  if (starting) await starting;
  if (current && currentScore !== null && Number.isFinite(currentScore)) {
    current.freeze();
    await recordResult(current.id, currentScore, performance.now() - startedAt, current.lowerIsBetter);
    currentScore = null;
  }
  await requestHide();
}

window.addEventListener("keydown", (event) => {
  if (event.key === "Escape") {
    event.preventDefault();
    void dismissNow();
    return;
  }
  if (event.key === "Tab" && !event.ctrlKey && !event.metaKey && !event.altKey) {
    event.preventDefault();
    void cycleGame(event.shiftKey ? -1 : 1);
  }
});

void gameTabs.refresh();

void listen<ShowPayload>("game:show", (payload) => {
  void startGame(payload.gameId);
});

void listen("game:freeze", () => {
  void finishAndHide();
});

// Rust emits this right after the window is hidden, so the next show starts
// from a clean stage instead of resuming a dead run.
void listen("game:hide", () => {
  teardown();
  showingStats = false;
});

// Tray "Show stats".
void listen("view:stats", () => {
  void showStats();
});

installDevBar(installBrowserDriver(GAME_IDS), () => void showStats());

// If a hook fired while this webview was still loading, catch up on it.
void pendingGame().then((gameId) => {
  if (gameId && !current && !starting) void startGame(gameId);
});
