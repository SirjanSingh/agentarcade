import type { GameApi, GameModule } from "../../game";

const MIN_WAIT_MS = 1200;
const MAX_WAIT_MS = 4200;

type Phase = "idle" | "waiting" | "go" | "result" | "early";

/**
 * Wait for green, then click. Score is the best (lowest) reaction time of the
 * run — one unlucky twitch shouldn't define the result.
 */
export function createReaction(): GameModule {
  let api: GameApi;
  let pad: HTMLDivElement;
  let headline: HTMLParagraphElement;
  let detail: HTMLParagraphElement;
  let timer: number | undefined;
  let phase: Phase = "idle";
  let goAt = 0;
  let frozen = false;
  const times: number[] = [];

  function setPhase(next: Phase, head: string, sub: string): void {
    phase = next;
    pad.dataset.phase = next;
    headline.textContent = head;
    detail.textContent = sub;
  }

  function runSummary(): string {
    if (times.length === 0) return "click the pad to start";
    const best = Math.min(...times);
    const avg = times.reduce((a, b) => a + b, 0) / times.length;
    return `best ${Math.round(best)} ms · avg ${Math.round(avg)} ms · ${times.length} ${
      times.length === 1 ? "try" : "tries"
    }`;
  }

  function arm(): void {
    setPhase("waiting", "Wait for green…", runSummary());
    const delay = MIN_WAIT_MS + Math.random() * (MAX_WAIT_MS - MIN_WAIT_MS);
    timer = window.setTimeout(() => {
      goAt = performance.now();
      setPhase("go", "CLICK", runSummary());
    }, delay);
  }

  function onClick(): void {
    if (frozen) return;
    switch (phase) {
      case "idle":
      case "result":
      case "early":
        arm();
        break;
      case "waiting":
        // Clicked before green: penalise by making them re-arm, no score.
        window.clearTimeout(timer);
        setPhase("early", "Too soon", "click to try again");
        break;
      case "go": {
        const ms = performance.now() - goAt;
        times.push(ms);
        api.reportScore(Math.min(...times));
        setPhase("result", `${Math.round(ms)} ms`, `${runSummary()} · click to go again`);
        break;
      }
    }
  }

  return {
    id: "reaction",
    label: "Reaction",
    unit: "ms",
    lowerIsBetter: true,

    mount(root: HTMLElement, gameApi: GameApi): void {
      api = gameApi;
      pad = document.createElement("div");
      pad.className = "reaction-pad";
      headline = document.createElement("p");
      headline.className = "reaction-headline";
      detail = document.createElement("p");
      detail.className = "reaction-detail";
      pad.append(headline, detail);
      root.appendChild(pad);

      setPhase("idle", "Click to start", "then wait for green");
      pad.addEventListener("click", onClick);
    },

    freeze(): void {
      frozen = true;
      window.clearTimeout(timer);
      if (times.length > 0) {
        api.reportScore(Math.min(...times));
        setPhase("result", `${Math.round(Math.min(...times))} ms`, runSummary());
      } else {
        setPhase("idle", "No attempts", "that was a quick one");
      }
    },

    destroy(): void {
      frozen = true;
      window.clearTimeout(timer);
      pad?.removeEventListener("click", onClick);
      pad?.remove();
    },
  };
}
