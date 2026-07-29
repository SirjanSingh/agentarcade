import type { GameApi, GameModule } from "../../game";

/** Fixed tempo on purpose: a drifting BPM would make scores incomparable. */
const BPM = 100;
const PERIOD_MS = 60000 / BPM;
/** Below this the mean is one lucky tap rather than a measurement. */
const MIN_TAPS = 4;

/**
 * Tap Space on the beat. Score is mean absolute timing error in milliseconds,
 * lower better — a per-tap average, so it does not drift with run length.
 *
 * No audio: the app has no sound assets, and a visual pulse is enough to
 * entrain to.
 */
export function createMetronome(): GameModule {
  let api: GameApi;
  let canvas: HTMLCanvasElement;
  let ctx: CanvasRenderingContext2D;
  let meta: HTMLParagraphElement;
  let raf = 0;
  let frozen = false;

  let startedAt = 0;
  const errors: number[] = [];
  /** Timestamp and error of the most recent tap, for the ripple. */
  let lastTapAt = -Infinity;
  let lastError = 0;

  function meanError(): number {
    if (errors.length === 0) return 0;
    return errors.reduce((a, b) => a + b, 0) / errors.length;
  }

  function bestRun(): number | null {
    // Best window of 8 consecutive taps — rewards finding the groove.
    if (errors.length < 8) return null;
    let best = Infinity;
    for (let i = 0; i + 8 <= errors.length; i++) {
      const window = errors.slice(i, i + 8);
      best = Math.min(best, window.reduce((a, b) => a + b, 0) / 8);
    }
    return best;
  }

  function reportIfMeaningful(): void {
    if (errors.length >= MIN_TAPS) api.reportScore(meanError());
  }

  function onKey(event: KeyboardEvent): void {
    if (frozen || event.key !== " " || event.repeat) return;
    event.preventDefault();

    const now = performance.now();
    const elapsed = now - startedAt;
    // Distance to the nearest beat, whether we were early or late.
    const offset = elapsed % PERIOD_MS;
    const error = Math.min(offset, PERIOD_MS - offset);

    errors.push(error);
    lastTapAt = now;
    lastError = error;
    reportIfMeaningful();
  }

  function accentColour(error: number): string {
    if (error < 35) return "#7fd6a4";
    if (error < 80) return "#d8c26a";
    return "#e0704a";
  }

  function draw(now: number): void {
    const w = canvas.width;
    const h = canvas.height;
    const cx = w / 2;
    const cy = h / 2;

    ctx.clearRect(0, 0, w, h);

    const phase = ((now - startedAt) % PERIOD_MS) / PERIOD_MS;
    // Sharp attack, slow decay — reads as a pulse rather than a throb.
    const pulse = Math.pow(1 - phase, 3);

    ctx.strokeStyle = "rgba(255,255,255,0.10)";
    ctx.lineWidth = 2;
    ctx.beginPath();
    ctx.arc(cx, cy, 96, 0, Math.PI * 2);
    ctx.stroke();

    ctx.strokeStyle = "#7fd6a4";
    ctx.globalAlpha = 0.35 + pulse * 0.65;
    ctx.lineWidth = 3 + pulse * 5;
    ctx.beginPath();
    ctx.arc(cx, cy, 96, 0, Math.PI * 2);
    ctx.stroke();
    ctx.globalAlpha = 1;

    ctx.fillStyle = "#7fd6a4";
    ctx.globalAlpha = pulse;
    ctx.beginPath();
    ctx.arc(cx, cy, 12 + pulse * 10, 0, Math.PI * 2);
    ctx.fill();
    ctx.globalAlpha = 1;

    // Ripple from the last tap, coloured by how close it landed.
    const sinceTap = now - lastTapAt;
    if (sinceTap < 420) {
      const t = sinceTap / 420;
      ctx.strokeStyle = accentColour(lastError);
      ctx.globalAlpha = 1 - t;
      ctx.lineWidth = 2;
      ctx.beginPath();
      ctx.arc(cx, cy, 96 + t * 46, 0, Math.PI * 2);
      ctx.stroke();
      ctx.globalAlpha = 1;
    }

    ctx.fillStyle = "#8b909b";
    ctx.font = "12px ui-monospace, SFMono-Regular, Menlo, monospace";
    ctx.textAlign = "center";
    ctx.fillText(`${BPM} bpm`, cx, cy + 4);
  }

  function loop(now: number): void {
    raf = requestAnimationFrame(loop);
    if (!frozen) draw(now);
    updateMeta();
  }

  function updateMeta(): void {
    if (errors.length === 0) {
      meta.textContent = "tap space on the beat";
      return;
    }
    const best = bestRun();
    const parts = [`±${Math.round(meanError())} ms`, `${errors.length} taps`];
    if (best !== null) parts.push(`best 8: ±${Math.round(best)} ms`);
    if (errors.length < MIN_TAPS) parts.push(`${MIN_TAPS - errors.length} more to score`);
    meta.textContent = parts.join(" · ");
  }

  return {
    id: "metronome",
    label: "Metronome",
    unit: "ms",
    lowerIsBetter: true,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      const wrap = document.createElement("div");
      wrap.className = "metronome";
      canvas = document.createElement("canvas");
      canvas.width = 320;
      canvas.height = 320;
      canvas.className = "metronome-face";
      meta = document.createElement("p");
      meta.className = "metronome-meta";
      wrap.append(canvas, meta);
      root.appendChild(wrap);
      ctx = canvas.getContext("2d")!;

      startedAt = performance.now();
      window.addEventListener("keydown", onKey);
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
      window.removeEventListener("keydown", onKey);
      canvas?.parentElement?.remove();
    },
  };
}
