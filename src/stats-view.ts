import { formatScore } from "./game";
import { allGameIds, getGameFactory } from "./registry";
import type { GameStats, RunRecord } from "./types";

const SPARK_W = 74;
const SPARK_H = 20;

/** Tiny inline SVG trend line for a game's recent runs, oldest to newest. */
function sparkline(runs: RunRecord[], lowerIsBetter: boolean): SVGSVGElement {
  const svg = document.createElementNS("http://www.w3.org/2000/svg", "svg");
  svg.setAttribute("viewBox", `0 0 ${SPARK_W} ${SPARK_H}`);
  svg.setAttribute("class", "spark");

  if (runs.length < 2) return svg;

  const scores = runs.map((r) => r.score);
  const min = Math.min(...scores);
  const max = Math.max(...scores);
  const span = max - min || 1;

  const points = scores
    .map((score, i) => {
      const x = (i / (scores.length - 1)) * SPARK_W;
      // Always draw "better" upward, whichever direction better means.
      const normalised = (score - min) / span;
      const y = SPARK_H - (lowerIsBetter ? 1 - normalised : normalised) * (SPARK_H - 3) - 1.5;
      return `${x.toFixed(1)},${y.toFixed(1)}`;
    })
    .join(" ");

  const line = document.createElementNS("http://www.w3.org/2000/svg", "polyline");
  line.setAttribute("points", points);
  svg.appendChild(line);
  return svg;
}

function relativeTime(iso: string): string {
  const seconds = Math.max(0, (Date.now() - new Date(iso).getTime()) / 1000);
  if (seconds < 90) return "just now";
  const minutes = seconds / 60;
  if (minutes < 60) return `${Math.round(minutes)}m ago`;
  const hours = minutes / 60;
  if (hours < 24) return `${Math.round(hours)}h ago`;
  return `${Math.round(hours / 24)}d ago`;
}

function formatDuration(totalSeconds: number): string {
  if (totalSeconds < 60) return `${Math.round(totalSeconds)}s`;
  const minutes = Math.floor(totalSeconds / 60);
  if (minutes < 60) return `${minutes}m`;
  return `${Math.floor(minutes / 60)}h ${minutes % 60}m`;
}

/**
 * The screen microtaskrr forgot to build: it persists per-game bests and never
 * shows them anywhere.
 */
export function renderStats(root: HTMLElement, stats: GameStats): void {
  const wrap = document.createElement("div");
  wrap.className = "stats";

  const heading = document.createElement("p");
  heading.className = "stats-heading";
  heading.textContent = "your stats";
  wrap.appendChild(heading);

  const table = document.createElement("div");
  table.className = "stats-table";

  const played = allGameIds().filter((id) => stats.games[id]?.plays);
  if (played.length === 0) {
    const empty = document.createElement("p");
    empty.className = "stats-empty";
    empty.textContent = "no runs yet — play one and it shows up here";
    wrap.appendChild(empty);
  }

  for (const id of played) {
    const game = getGameFactory(id)?.();
    if (!game) continue;
    const entry = stats.games[id];
    const runs = stats.recent.filter((r) => r.gameId === id).slice(0, 12).reverse();

    const row = document.createElement("div");
    row.className = "stats-row";
    row.dataset.game = id;

    const name = document.createElement("span");
    name.className = "stats-name";
    name.textContent = game.label;

    const best = document.createElement("span");
    best.className = "stats-best";
    best.textContent = formatScore(game, entry.best);

    const detail = document.createElement("span");
    detail.className = "stats-detail";
    const average = entry.plays > 0 ? entry.total / entry.plays : 0;
    detail.textContent = `avg ${Math.round(average)} · ${entry.plays} run${entry.plays === 1 ? "" : "s"}`;

    row.append(name, best, detail, sparkline(runs, game.lowerIsBetter));
    table.appendChild(row);
  }
  wrap.appendChild(table);

  const totals = document.createElement("p");
  totals.className = "stats-totals";
  totals.textContent = `${stats.totals.sessions} run${
    stats.totals.sessions === 1 ? "" : "s"
  } · ${formatDuration(stats.totals.secondsPlayed)} of waiting filled`;
  wrap.appendChild(totals);

  if (stats.recent.length > 0) {
    const recentHeading = document.createElement("p");
    recentHeading.className = "stats-subheading";
    recentHeading.textContent = "recent";
    wrap.appendChild(recentHeading);

    const list = document.createElement("ul");
    list.className = "stats-recent";
    for (const run of stats.recent.slice(0, 6)) {
      const game = getGameFactory(run.gameId)?.();
      const item = document.createElement("li");
      const label = document.createElement("span");
      label.textContent = game ? game.label : run.gameId;
      const score = document.createElement("span");
      score.className = "stats-recent-score";
      score.textContent = game ? formatScore(game, run.score) : String(Math.round(run.score));
      const when = document.createElement("span");
      when.className = "stats-recent-when";
      when.textContent = relativeTime(run.at);
      item.append(label, score, when);
      list.appendChild(item);
    }
    wrap.appendChild(list);
  }

  const footer = document.createElement("p");
  footer.className = "stats-footer";
  footer.textContent = "stored locally · nothing leaves this machine";
  wrap.appendChild(footer);

  root.replaceChildren(wrap);
}
