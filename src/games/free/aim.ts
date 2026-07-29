import type { GameApi, GameModule } from "../../game";

const START_RADIUS = 34;
const MIN_RADIUS = 15;
/** Every this many hits the target shrinks a little. */
const SHRINK_EVERY = 5;
/** A miss costs this much wall-clock, so spraying is worse than aiming. */
const MISS_PENALTY_MS = 250;
const MIN_HITS = 3;

interface Target {
  x: number;
  y: number;
  r: number;
  bornAt: number;
}

/**
 * Click the target, the next one appears. Scored as targets per minute rather
 * than raw hits: raw hits would just count how long Claude took to answer.
 */
export function createAim(): GameModule {
  let api: GameApi;
  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let meta: HTMLParagraphElement;
  let raf = 0;
  let frozen = false;

  let target: Target | null = null;
  let hits = 0;
  let misses = 0;
  let startedAt = 0;
  let penaltyMs = 0;
  let lastHitAt = -Infinity;
  let lastHitMs = 0;

  function radius(): number {
    return Math.max(MIN_RADIUS, START_RADIUS - Math.floor(hits / SHRINK_EVERY) * 3);
  }

  function spawn(): void {
    const r = radius();
    const pad = r + 8;
    target = {
      x: pad + Math.random() * (canvas.width - pad * 2),
      y: pad + Math.random() * (canvas.height - pad * 2),
      r,
      bornAt: performance.now(),
    };
  }

  function elapsedMs(): number {
    return performance.now() - startedAt + penaltyMs;
  }

  function targetsPerMinute(): number {
    const elapsed = elapsedMs();
    if (elapsed <= 0) return 0;
    return hits / (elapsed / 60000);
  }

  function accuracy(): number {
    const shots = hits + misses;
    return shots === 0 ? 100 : (hits / shots) * 100;
  }

  function reportIfMeaningful(): void {
    if (hits >= MIN_HITS) api.reportScore(targetsPerMinute());
  }

  function onClick(event: MouseEvent): void {
    if (frozen || !target) return;
    const rect = canvas.getBoundingClientRect();
    // The canvas is CSS-scaled to fit the stage, so map back to canvas pixels.
    const x = ((event.clientX - rect.left) / rect.width) * canvas.width;
    const y = ((event.clientY - rect.top) / rect.height) * canvas.height;

    const dx = x - target.x;
    const dy = y - target.y;
    if (dx * dx + dy * dy <= target.r * target.r) {
      lastHitMs = performance.now() - target.bornAt;
      lastHitAt = performance.now();
      hits += 1;
      reportIfMeaningful();
      spawn();
    } else {
      misses += 1;
      penaltyMs += MISS_PENALTY_MS;
    }
  }

  function draw(now: number): void {
    ctx.fillStyle = "#12141a";
    ctx.fillRect(0, 0, canvas.width, canvas.height);

    if (target) {
      // A brief grow-in makes the spawn readable instead of a hard pop.
      const age = Math.min(1, (now - target.bornAt) / 90);
      const r = target.r * (0.7 + age * 0.3);

      ctx.fillStyle = "rgba(127, 214, 164, 0.16)";
      ctx.beginPath();
      ctx.arc(target.x, target.y, r, 0, Math.PI * 2);
      ctx.fill();

      ctx.strokeStyle = "#7fd6a4";
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(target.x, target.y, r, 0, Math.PI * 2);
      ctx.stroke();

      ctx.fillStyle = "#7fd6a4";
      ctx.beginPath();
      ctx.arc(target.x, target.y, 3, 0, Math.PI * 2);
      ctx.fill();
    }

    const sinceHit = now - lastHitAt;
    if (sinceHit < 500) {
      ctx.fillStyle = "#7fd6a4";
      ctx.globalAlpha = 1 - sinceHit / 500;
      ctx.font = "600 13px ui-monospace, SFMono-Regular, Menlo, monospace";
      ctx.textAlign = "center";
      ctx.fillText(`${Math.round(lastHitMs)} ms`, canvas.width / 2, 22);
      ctx.globalAlpha = 1;
    }
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (frozen) return;
    draw(now);
    updateMeta();
  }

  function updateMeta(): void {
    if (hits === 0 && misses === 0) {
      meta.textContent = "click the target";
      return;
    }
    const parts = [`${Math.round(targetsPerMinute())} /min`, `${hits} hit`, `${Math.round(accuracy())}% acc`];
    if (hits < MIN_HITS) parts.push(`${MIN_HITS - hits} more to score`);
    meta.textContent = parts.join(" · ");
  }

  return {
    id: "aim",
    label: "Aim",
    unit: "/min",
    lowerIsBetter: false,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      const wrap = document.createElement("div");
      wrap.className = "aim";
      canvas = document.createElement("canvas");
      canvas.width = 470;
      canvas.height = 400;
      canvas.className = "board aim-board";
      meta = document.createElement("p");
      meta.className = "aim-meta";
      wrap.append(canvas, meta);
      root.appendChild(wrap);
      ctx = canvas.getContext("2d")!;

      startedAt = performance.now();
      spawn();
      canvas.addEventListener("click", onClick);
      raf = requestAnimationFrame(loop);
    },

    freeze(): void {
      frozen = true;
      reportIfMeaningful();
      updateMeta();
    },

    destroy(): void {
      frozen = true;
      cancelAnimationFrame(raf);
      canvas?.removeEventListener("click", onClick);
      canvas?.parentElement?.remove();
    },
  };
}
