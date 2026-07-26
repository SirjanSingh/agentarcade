import type { GameApi, GameModule } from "../game";

const COLS = 5;
const ROWS = 6;

interface Hsl {
  h: number;
  s: number;
  l: number;
}

/** Bilinear blend of the four corner colours, so the board is a smooth field. */
function blend(corners: [Hsl, Hsl, Hsl, Hsl], u: number, v: number): Hsl {
  const [tl, tr, bl, br] = corners;
  const mix = (a: number, b: number, t: number) => a + (b - a) * t;
  const top = { h: mix(tl.h, tr.h, u), s: mix(tl.s, tr.s, u), l: mix(tl.l, tr.l, u) };
  const bottom = { h: mix(bl.h, br.h, u), s: mix(bl.s, br.s, u), l: mix(bl.l, br.l, u) };
  return {
    h: mix(top.h, bottom.h, v),
    s: mix(top.s, bottom.s, v),
    l: mix(top.l, bottom.l, v),
  };
}

function css({ h, s, l }: Hsl): string {
  return `hsl(${h.toFixed(1)} ${s.toFixed(1)}% ${l.toFixed(1)}%)`;
}

function randomCorners(): [Hsl, Hsl, Hsl, Hsl] {
  const base = Math.random() * 360;
  // A narrow hue sweep plus a strong lightness fall is what makes a shuffled
  // board still read as one coherent field. A wide sweep just looks like
  // scattered confetti, and there is nothing to restore by eye.
  const spread = 40 + Math.random() * 35;
  return [
    { h: base, s: 58, l: 74 },
    { h: base + spread, s: 66, l: 62 },
    { h: base + spread * 0.4, s: 52, l: 44 },
    { h: base + spread * 1.15, s: 60, l: 32 },
  ];
}

/** Index is the cell a tile currently sits in; value is where it belongs. */
type Board = number[];

function isAnchor(cell: number): boolean {
  const row = Math.floor(cell / COLS);
  const col = cell % COLS;
  return (row === 0 || row === ROWS - 1) && (col === 0 || col === COLS - 1);
}

/**
 * Restore a shuffled colour gradient by swapping tiles.
 *
 * Scored as the best sortedness reached this run, which is bounded at 100 and
 * defined at every frame — being frozen three seconds in gives an honest
 * partial number instead of a meaningless one.
 */
export function createGradient(): GameModule {
  let api: GameApi;
  let wrap: HTMLDivElement;
  let grid: HTMLDivElement;
  let meta: HTMLParagraphElement;
  let colours: string[] = [];
  let board: Board = [];
  let selected: number | null = null;
  let bestPercent = 0;
  let cleared = 0;
  let frozen = false;

  function newBoard(): void {
    const corners = randomCorners();
    colours = [];
    for (let cell = 0; cell < COLS * ROWS; cell++) {
      const u = (cell % COLS) / (COLS - 1);
      const v = Math.floor(cell / COLS) / (ROWS - 1);
      colours.push(css(blend(corners, u, v)));
    }

    board = colours.map((_, i) => i);
    const movable = board.filter((cell) => !isAnchor(cell));
    // Fisher-Yates over the movable cells only; corners stay put as reference
    // points, which is what makes the puzzle solvable by eye.
    for (let i = movable.length - 1; i > 0; i--) {
      const j = Math.floor(Math.random() * (i + 1));
      const a = movable[i];
      const b = movable[j];
      [board[a], board[b]] = [board[b], board[a]];
    }
    selected = null;
    render();
  }

  function distance(cellA: number, cellB: number): number {
    const ax = cellA % COLS;
    const ay = Math.floor(cellA / COLS);
    const bx = cellB % COLS;
    const by = Math.floor(cellB / COLS);
    return Math.abs(ax - bx) + Math.abs(ay - by);
  }

  /** The furthest any tile could sit from where it belongs. */
  function worstDistance(cell: number): number {
    const corners = [0, COLS - 1, (ROWS - 1) * COLS, ROWS * COLS - 1];
    return Math.max(...corners.map((corner) => distance(cell, corner)));
  }

  /**
   * How close the board is to solved, by total tile displacement rather than
   * by exact matches. Counting only tiles in their home cell is far too sparse
   * on a 26-tile board — a shuffle starts near 0% and real progress barely
   * moves it, so the score would tell the player nothing until the very end.
   */
  function sortedness(): number {
    let displaced = 0;
    let worst = 0;
    for (let cell = 0; cell < board.length; cell++) {
      if (isAnchor(cell)) continue;
      displaced += distance(cell, board[cell]);
      worst += worstDistance(cell);
    }
    return worst === 0 ? 100 : (1 - displaced / worst) * 100;
  }

  function render(): void {
    grid.replaceChildren();
    for (let cell = 0; cell < board.length; cell++) {
      const tile = document.createElement("button");
      tile.type = "button";
      tile.className = "hue-tile";
      tile.style.background = colours[board[cell]];
      if (isAnchor(cell)) tile.classList.add("anchor");
      if (selected === cell) tile.classList.add("selected");
      tile.addEventListener("click", () => onPick(cell));
      grid.appendChild(tile);
    }
    const percent = sortedness();
    meta.textContent = frozen
      ? `${Math.round(bestPercent)}% sorted`
      : `${Math.round(percent)}% sorted${cleared > 0 ? ` · ${cleared} cleared` : ""}`;
  }

  function onPick(cell: number): void {
    if (frozen || isAnchor(cell)) return;
    if (selected === null) {
      selected = cell;
      render();
      return;
    }
    if (selected === cell) {
      selected = null;
      render();
      return;
    }

    [board[selected], board[cell]] = [board[cell], board[selected]];
    selected = null;

    const percent = sortedness();
    if (percent > bestPercent) {
      bestPercent = percent;
      api.reportScore(bestPercent);
    }
    render();

    if (percent >= 99.99) {
      cleared += 1;
      wrap.dataset.state = "cleared";
      window.setTimeout(() => {
        if (frozen) return;
        wrap.dataset.state = "";
        newBoard();
      }, 700);
    }
  }

  return {
    id: "gradient",
    label: "Gradient",
    unit: "%",
    lowerIsBetter: false,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      wrap = document.createElement("div");
      wrap.className = "hue";
      grid = document.createElement("div");
      grid.className = "hue-grid";
      grid.style.gridTemplateColumns = `repeat(${COLS}, 1fr)`;
      meta = document.createElement("p");
      meta.className = "hue-meta";
      wrap.append(grid, meta);
      root.appendChild(wrap);

      newBoard();
    },

    freeze(): void {
      frozen = true;
      render();
    },

    destroy(): void {
      frozen = true;
      wrap?.remove();
    },
  };
}
