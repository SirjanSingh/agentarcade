import type { GameApi, GameModule } from "../game";

/**
 * Indiana Jones endless runner, Claude-flavoured. You flee a rolling Anthropic
 * sunburst down a temple corridor, hopping blocks of code. The world scrolls at
 * a speed that ramps hard with distance; the boulder chases from behind as a gap
 * that shrinks on every clip and barely recovers when you run clean.
 *
 * Score is metres per minute — average speed. Like snake's apples/min it is a
 * rate, so it is defined at every frozen instant and does not drift with run
 * length: a longer wait spent stumbling scores worse, not better. Dying burns
 * time without distance, which is the whole penalty.
 */

const WIDTH = 500;
const HEIGHT = 420;
const GROUND_Y = HEIGHT - 66;
const RUNNER_X = 148;
const RUNNER_HW = 12; // half-width of the runner's collision box
const RUNNER_H = 34;

const SPEED0 = 250; // px/sec at the start of a life
const SPEED_MAX = 660;
const SPEED_RAMP = 0.95; // px/sec added per metre travelled — steep, gets brutal
const PX_PER_METRE = 22;

const GRAVITY = 2250; // px/sec^2
const JUMP_V = 720; // px/sec initial upward velocity

// Boulder is a gap, in px, between its leading edge and the runner's back.
const LEAD_START = 176;
const LEAD_MAX = 200;
const HIT_PENALTY = 94; // gap lost per obstacle clipped — two misses is nearly fatal
const RECOVER_RATE = 13; // px/sec the boulder falls back while running clean
const BOULDER_R = 46;

const SPAWN_MIN = 0.68; // seconds between obstacles
const SPAWN_MAX = 1.3;
const CLUSTER_CHANCE = 0.32; // chance an obstacle is trailed closely by another
const CLUSTER_GAP = 0.22; // seconds — clearable inside a single jump arc
const MIN_SCORING_MS = 6000; // below this m/min is dominated by the first steps

const TOKENS = ["{ }", "</>", "0xFF", "NaN", "==", "sudo", "404", "&&", "null", "[]", "#!", "git", ";;", "%s", "=>"];
const TOKEN_COLORS = ["#7fd6a4", "#e8c07a", "#8ab4f8", "#e0704a", "#c792ea", "#89ddff"];

interface Obstacle {
  x: number;
  w: number;
  h: number;
  token: string;
  color: string;
  hit: boolean;
}

export function createBoulder(): GameModule {
  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let api: GameApi;
  let raf = 0;
  let frozen = false;
  let dead = false;

  // Per-run, persists across lives (the scored quantities).
  let totalDistancePx = 0;
  let runStart = 0;

  // Per-life state.
  let speed = SPEED0;
  let lead = LEAD_START;
  let obstacles: Obstacle[] = [];
  let spawnTimer = 0;
  let justCluster = false;
  let runnerLift = 0; // px above ground, 0 = grounded
  let runnerVy = 0; // px/sec, positive = rising
  let legPhase = 0; // running animation
  let shake = 0; // dust/impact shake, decays
  let lastNow = 0;

  function resetLife(): void {
    speed = SPEED0;
    lead = LEAD_START;
    obstacles = [];
    spawnTimer = 0.6;
    justCluster = false;
    runnerLift = 0;
    runnerVy = 0;
    shake = 0;
    dead = false;
  }

  function metresPerMinute(): number {
    const elapsed = performance.now() - runStart;
    if (elapsed < MIN_SCORING_MS) return 0;
    const metres = totalDistancePx / PX_PER_METRE;
    return metres / (elapsed / 60000);
  }

  function reportIfMeaningful(): void {
    if (performance.now() - runStart >= MIN_SCORING_MS) api.reportScore(metresPerMinute());
  }

  function jump(): void {
    if (runnerLift <= 0.5) runnerVy = JUMP_V;
  }

  function spawn(): void {
    const token = TOKENS[Math.floor(Math.random() * TOKENS.length)];
    const color = TOKEN_COLORS[Math.floor(Math.random() * TOKEN_COLORS.length)];
    const w = 26 + token.length * 9;
    const h = 28 + Math.random() * 20;
    obstacles.push({ x: WIDTH + w, w, h, token, color, hit: false });
    if (!justCluster && Math.random() < CLUSTER_CHANCE) {
      spawnTimer = CLUSTER_GAP;
      justCluster = true;
    } else {
      spawnTimer = SPAWN_MIN + Math.random() * (SPAWN_MAX - SPAWN_MIN);
      justCluster = false;
    }
  }

  function update(dt: number): void {
    speed = Math.min(SPEED_MAX, SPEED0 + (totalDistancePx / PX_PER_METRE) * SPEED_RAMP);
    const advance = speed * dt;
    totalDistancePx += advance;
    legPhase += advance * 0.05;
    if (shake > 0) shake = Math.max(0, shake - dt * 60);

    // Jump arc.
    runnerVy -= GRAVITY * dt;
    runnerLift += runnerVy * dt;
    if (runnerLift <= 0) {
      runnerLift = 0;
      runnerVy = 0;
    }

    // Obstacles march left; spawn on a timer so spacing stays clearable.
    spawnTimer -= dt;
    if (spawnTimer <= 0) spawn();
    for (const ob of obstacles) ob.x -= advance;
    obstacles = obstacles.filter((ob) => ob.x + ob.w > -20);

    // Collision: clipped once each, brings the boulder forward.
    for (const ob of obstacles) {
      if (ob.hit) continue;
      const overlapX = ob.x < RUNNER_X + RUNNER_HW && ob.x + ob.w > RUNNER_X - RUNNER_HW;
      if (overlapX && runnerLift < ob.h) {
        ob.hit = true;
        lead -= HIT_PENALTY;
        shake = 10;
      }
    }

    // Boulder recovers slowly while you run clean; catches you when it closes.
    lead = Math.min(LEAD_MAX, lead + RECOVER_RATE * dt);
    if (lead <= 0) {
      dead = true;
      shake = 15;
    }
    reportIfMeaningful();
  }

  function drawRunner(ground: number): void {
    const feet = ground - runnerLift;
    const airborne = runnerLift > 1;
    ctx.save();
    ctx.translate(RUNNER_X, feet);
    ctx.fillStyle = "#e8c07a"; // warm khaki silhouette
    ctx.fillRect(-8, -RUNNER_H, 16, RUNNER_H - 8);
    ctx.beginPath();
    ctx.arc(0, -RUNNER_H - 4, 7, 0, Math.PI * 2);
    ctx.fill();
    ctx.fillStyle = "#8a5a2b"; // hat
    ctx.fillRect(-11, -RUNNER_H - 8, 22, 4);
    ctx.fillRect(-6, -RUNNER_H - 14, 12, 7);
    ctx.fillStyle = "#c79a54"; // legs
    if (airborne) {
      ctx.fillRect(-8, -10, 7, 10);
      ctx.fillRect(2, -10, 7, 8);
    } else {
      const swing = Math.sin(legPhase) * 6;
      ctx.fillRect(-8 - swing * 0.4, -8, 6, 8 + swing);
      ctx.fillRect(3 + swing * 0.4, -8, 6, 8 - swing);
    }
    ctx.restore();
  }

  /** The Anthropic sunburst: tapered spokes radiating from the centre. */
  function drawBoulder(ground: number): void {
    const cx = RUNNER_X - RUNNER_HW - lead - BOULDER_R;
    const cy = ground - BOULDER_R + 6;
    const rot = totalDistancePx * 0.018; // rolls as it chases
    ctx.save();
    ctx.translate(cx, cy);

    // Soft glow behind the mark.
    const glow = ctx.createRadialGradient(0, 0, 4, 0, 0, BOULDER_R + 8);
    glow.addColorStop(0, "rgba(217,119,87,0.35)");
    glow.addColorStop(1, "rgba(217,119,87,0)");
    ctx.fillStyle = glow;
    ctx.beginPath();
    ctx.arc(0, 0, BOULDER_R + 8, 0, Math.PI * 2);
    ctx.fill();

    ctx.rotate(rot);
    ctx.fillStyle = "#d97757"; // Claude terracotta
    const spokes = 12;
    const inner = BOULDER_R * 0.16;
    const outer = BOULDER_R;
    const half = (Math.PI / spokes) * 0.42; // spoke half-width in radians
    for (let i = 0; i < spokes; i++) {
      const a = (i * Math.PI * 2) / spokes;
      ctx.beginPath();
      ctx.moveTo(Math.cos(a - half) * inner, Math.sin(a - half) * inner);
      ctx.lineTo(Math.cos(a) * outer, Math.sin(a) * outer);
      ctx.lineTo(Math.cos(a + half) * inner, Math.sin(a + half) * inner);
      ctx.closePath();
      ctx.fill();
    }
    ctx.beginPath();
    ctx.arc(0, 0, inner * 1.4, 0, Math.PI * 2);
    ctx.fill();
    ctx.restore();
  }

  function drawObstacle(ob: Obstacle, ground: number): void {
    const y = ground - ob.h;
    const r = 5;
    ctx.fillStyle = "#14110d";
    ctx.strokeStyle = ob.color;
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.moveTo(ob.x + r, y);
    ctx.arcTo(ob.x + ob.w, y, ob.x + ob.w, y + ob.h, r);
    ctx.arcTo(ob.x + ob.w, y + ob.h, ob.x, y + ob.h, r);
    ctx.arcTo(ob.x, y + ob.h, ob.x, y, r);
    ctx.arcTo(ob.x, y, ob.x + ob.w, y, r);
    ctx.closePath();
    ctx.fill();
    ctx.stroke();
    ctx.fillStyle = ob.color;
    ctx.font = "600 15px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.textAlign = "center";
    ctx.textBaseline = "middle";
    ctx.fillText(ob.token, ob.x + ob.w / 2, y + ob.h / 2 + 1);
    ctx.textBaseline = "alphabetic";
  }

  function draw(): void {
    const shakeX = shake > 0 ? (Math.random() - 0.5) * shake : 0;
    const shakeY = shake > 0 ? (Math.random() - 0.5) * shake : 0;
    ctx.save();
    ctx.translate(shakeX, shakeY);

    const bg = ctx.createLinearGradient(0, 0, 0, HEIGHT);
    bg.addColorStop(0, "#2a2018");
    bg.addColorStop(1, "#1a130d");
    ctx.fillStyle = bg;
    ctx.fillRect(-20, -20, WIDTH + 40, HEIGHT + 40);

    // Scrolling stone floor.
    ctx.fillStyle = "#3a2c1d";
    ctx.fillRect(-20, GROUND_Y, WIDTH + 40, HEIGHT - GROUND_Y + 20);
    ctx.strokeStyle = "rgba(0,0,0,0.35)";
    ctx.lineWidth = 2;
    const slab = 46;
    const offset = totalDistancePx % slab;
    for (let x = -offset; x < WIDTH + slab; x += slab) {
      ctx.beginPath();
      ctx.moveTo(x, GROUND_Y);
      ctx.lineTo(x - 14, HEIGHT);
      ctx.stroke();
    }
    ctx.strokeStyle = "rgba(232,192,122,0.25)";
    ctx.beginPath();
    ctx.moveTo(-20, GROUND_Y + 0.5);
    ctx.lineTo(WIDTH + 20, GROUND_Y + 0.5);
    ctx.stroke();

    for (const ob of obstacles) drawObstacle(ob, GROUND_Y);
    drawBoulder(GROUND_Y);
    drawRunner(GROUND_Y);

    ctx.restore();

    if (dead && !frozen) {
      ctx.fillStyle = "rgba(9,7,4,0.8)";
      ctx.fillRect(0, HEIGHT / 2 - 44, WIDTH, 88);
      ctx.fillStyle = "#f2e6d0";
      ctx.font = "600 20px ui-monospace, SFMono-Regular, Menlo, monospace";
      ctx.textAlign = "center";
      ctx.fillText(`${Math.round(metresPerMinute())} m/min`, WIDTH / 2, HEIGHT / 2 - 4);
      ctx.fillStyle = "#b09873";
      ctx.font = "13px ui-monospace, SFMono-Regular, Menlo, monospace";
      ctx.fillText("the sunburst got you · space to run on", WIDTH / 2, HEIGHT / 2 + 22);
    }
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (frozen) return;
    const dt = Math.min(0.05, (now - lastNow) / 1000);
    lastNow = now;
    if (!dead) update(dt);
    draw();
  }

  function onKey(event: KeyboardEvent): void {
    if (frozen) return;
    const isJump = event.key === " " || event.key === "ArrowUp" || event.key === "w" || event.key === "W";
    if (!isJump) return;
    event.preventDefault();
    if (dead) resetLife();
    else jump();
  }

  function onPointer(event: PointerEvent): void {
    if (frozen) return;
    event.preventDefault();
    if (dead) resetLife();
    else jump();
  }

  return {
    id: "boulder",
    label: "Boulder",
    unit: "m/min",
    lowerIsBetter: false,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      runStart = performance.now();
      totalDistancePx = 0;
      canvas = document.createElement("canvas");
      canvas.width = WIDTH;
      canvas.height = HEIGHT;
      canvas.className = "board";
      root.appendChild(canvas);
      ctx = canvas.getContext("2d")!;

      resetLife();
      window.addEventListener("keydown", onKey);
      canvas.addEventListener("pointerdown", onPointer);
      lastNow = performance.now();
      raf = requestAnimationFrame(loop);
    },

    freeze(): void {
      frozen = true;
      reportIfMeaningful();
      draw();
    },

    destroy(): void {
      frozen = true;
      cancelAnimationFrame(raf);
      window.removeEventListener("keydown", onKey);
      canvas?.removeEventListener("pointerdown", onPointer);
      canvas?.remove();
    },
  };
}
