import { formatScore, isBetter, type GameModule } from "./game";
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

let current: GameModule | null = null;
let currentScore: number | null = null;
let currentBest: number | null = null;
let startedAt = 0;
let hideTimer: number | undefined;
let showingStats = false;

async function showStats(): Promise<void> {
  teardown();
  showingStats = true;
  label.textContent = "Stats";
  bestEl.textContent = "";
  waitingEl.hidden = true;
  renderStats(stage, await getStats());
}

async function startGame(gameId: string): Promise<void> {
  teardown();
  showingStats = false;

  const factory = REGISTRY[gameId] ?? REGISTRY[GAME_IDS[0]];
  const game = factory();
  current = game;
  currentScore = null;
  startedAt = performance.now();

  const stats = await getStats();
  currentBest = stats.games[game.id]?.best ?? null;

  label.textContent = game.label;
  bestEl.textContent = currentBest === null ? "no personal best yet" : `best ${formatScore(game, currentBest)}`;
  waitingEl.hidden = false;
  scorecard.hidden = true;

  game.mount(stage, {
    reportScore(score: number) {
      currentScore = score;
    },
    get best() {
      return currentBest;
    },
  });
}

/** Claude finished: stop the run, show the result, then get out of the way. */
async function finishAndHide(): Promise<void> {
  // Nothing to score if the user was reading their stats when Claude finished.
  if (!current || showingStats) {
    await requestHide();
    return;
  }
  const game = current;
  game.freeze();
  waitingEl.hidden = true;

  if (currentScore !== null) {
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
  currentScore = null;
  stage.replaceChildren();
  scorecard.hidden = true;
}

/** Esc dismisses immediately, but the run still counts. */
async function dismissNow(): Promise<void> {
  if (current && currentScore !== null) {
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
  }
});

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
  if (gameId && !current) void startGame(gameId);
});
