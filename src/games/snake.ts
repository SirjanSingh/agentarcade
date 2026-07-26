import type { GameApi, GameModule } from "../game";

const CELL = 20;
const COLS = 25;
const ROWS = 21;
const START_TICK_MS = 130;
const MIN_TICK_MS = 70;

interface Point {
  x: number;
  y: number;
}

const OPPOSITE: Record<string, string> = { up: "down", down: "up", left: "right", right: "left" };
const VECTORS: Record<string, Point> = {
  up: { x: 0, y: -1 },
  down: { x: 0, y: 1 },
  left: { x: -1, y: 0 },
  right: { x: 1, y: 0 },
};

const KEYS: Record<string, string> = {
  ArrowUp: "up",
  ArrowDown: "down",
  ArrowLeft: "left",
  ArrowRight: "right",
  w: "up",
  s: "down",
  a: "left",
  d: "right",
  W: "up",
  S: "down",
  A: "left",
  D: "right",
};

export function createSnake(): GameModule {
  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let api: GameApi;
  let raf = 0;
  let frozen = false;
  let dead = false;

  let snake: Point[] = [];
  let direction = "right";
  /** Queued turns, so two fast key presses in one tick both register. */
  let pending: string[] = [];
  let food: Point = { x: 0, y: 0 };
  let score = 0;
  let tickMs = START_TICK_MS;
  let lastTick = 0;

  function reset(): void {
    const midY = Math.floor(ROWS / 2);
    snake = [
      { x: 4, y: midY },
      { x: 3, y: midY },
      { x: 2, y: midY },
    ];
    direction = "right";
    pending = [];
    score = 0;
    tickMs = START_TICK_MS;
    dead = false;
    placeFood();
    api.reportScore(0);
  }

  function placeFood(): void {
    // Rejection sampling is fine: the board is never close to full in practice.
    let candidate: Point;
    do {
      candidate = { x: Math.floor(Math.random() * COLS), y: Math.floor(Math.random() * ROWS) };
    } while (snake.some((s) => s.x === candidate.x && s.y === candidate.y));
    food = candidate;
  }

  function step(): void {
    const next = pending.shift();
    if (next && OPPOSITE[direction] !== next) direction = next;

    const vec = VECTORS[direction];
    const head = { x: snake[0].x + vec.x, y: snake[0].y + vec.y };

    const hitWall = head.x < 0 || head.y < 0 || head.x >= COLS || head.y >= ROWS;
    const hitSelf = snake.some((s) => s.x === head.x && s.y === head.y);
    if (hitWall || hitSelf) {
      dead = true;
      return;
    }

    snake.unshift(head);
    if (head.x === food.x && head.y === food.y) {
      score += 1;
      api.reportScore(score);
      tickMs = Math.max(MIN_TICK_MS, tickMs - 3);
      placeFood();
    } else {
      snake.pop();
    }
  }

  function draw(): void {
    const w = canvas.width;
    const h = canvas.height;
    ctx.fillStyle = "#12141a";
    ctx.fillRect(0, 0, w, h);

    ctx.strokeStyle = "rgba(255,255,255,0.035)";
    ctx.lineWidth = 1;
    for (let x = CELL; x < w; x += CELL) {
      ctx.beginPath();
      ctx.moveTo(x + 0.5, 0);
      ctx.lineTo(x + 0.5, h);
      ctx.stroke();
    }
    for (let y = CELL; y < h; y += CELL) {
      ctx.beginPath();
      ctx.moveTo(0, y + 0.5);
      ctx.lineTo(w, y + 0.5);
      ctx.stroke();
    }

    ctx.fillStyle = "#e0704a";
    ctx.beginPath();
    ctx.arc(food.x * CELL + CELL / 2, food.y * CELL + CELL / 2, CELL / 2 - 3, 0, Math.PI * 2);
    ctx.fill();

    for (let i = snake.length - 1; i >= 0; i--) {
      const seg = snake[i];
      const fade = 1 - (i / snake.length) * 0.55;
      ctx.fillStyle = i === 0 ? "#7fd6a4" : `rgba(97, 186, 140, ${fade.toFixed(3)})`;
      ctx.fillRect(seg.x * CELL + 1, seg.y * CELL + 1, CELL - 2, CELL - 2);
    }

    if (dead && !frozen) {
      ctx.fillStyle = "rgba(9,10,13,0.78)";
      ctx.fillRect(0, h / 2 - 44, w, 88);
      ctx.fillStyle = "#f2f3f5";
      ctx.font = "600 20px ui-monospace, SFMono-Regular, Menlo, monospace";
      ctx.textAlign = "center";
      ctx.fillText(`${score} point${score === 1 ? "" : "s"}`, w / 2, h / 2 - 4);
      ctx.fillStyle = "#8b909b";
      ctx.font = "13px ui-monospace, SFMono-Regular, Menlo, monospace";
      ctx.fillText("space to play again", w / 2, h / 2 + 22);
    }
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (frozen) return;
    if (!dead && now - lastTick >= tickMs) {
      lastTick = now;
      step();
    }
    draw();
  }

  function onKey(event: KeyboardEvent): void {
    if (frozen) return;
    if (event.key === " " && dead) {
      event.preventDefault();
      reset();
      return;
    }
    const dir = KEYS[event.key];
    if (!dir) return;
    event.preventDefault();
    // Cap the queue so mashing keys can't buffer a long turn sequence.
    if (pending.length < 2) pending.push(dir);
  }

  return {
    id: "snake",
    label: "Snake",
    unit: "pts",
    lowerIsBetter: false,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      canvas = document.createElement("canvas");
      canvas.width = COLS * CELL;
      canvas.height = ROWS * CELL;
      canvas.className = "board";
      root.appendChild(canvas);
      ctx = canvas.getContext("2d")!;

      reset();
      window.addEventListener("keydown", onKey);
      lastTick = performance.now();
      raf = requestAnimationFrame(loop);
    },

    freeze(): void {
      frozen = true;
      draw();
    },

    destroy(): void {
      frozen = true;
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onKey);
      canvas?.remove();
    },
  };
}
