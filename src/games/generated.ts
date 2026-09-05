import type { GameApi, GameFactory, GameModule } from "../game";
import type { Pack } from "../types";

/**
 * Wraps a generated game — one HTML file written by the user's coding agent —
 * in the same `GameModule` contract the built-in games implement. Everything
 * downstream (the score card, freeze-on-Stop, stats, the tab bar) therefore
 * treats it as an ordinary game and needs to know nothing about where it came
 * from.
 *
 * The game runs in an iframe with `allow-scripts` and deliberately *without*
 * `allow-same-origin`, which leaves it on an opaque origin: no reach into this
 * document, no storage. Rust serves it its own Content-Security-Policy on top
 * of that, so a generated `fetch()` fails even if the model writes one. See
 * src-tauri/src/packs.rs.
 *
 * Tauri injects `__TAURI_INTERNALS__` into every frame, so it is visible from
 * in there and cannot be deleted from this side. It is inert: a command call
 * from that frame is rejected and never executes, because capabilities are not
 * granted to a remote origin. Verified by probing from inside the sandbox
 * rather than assumed — if a capability is ever given a `remote.urls` entry,
 * that reasoning needs rechecking.
 *
 * The only channel is postMessage, in two directions and two message types.
 */

/** Sent by the game, as often as it likes. The shell keeps the latest. */
const SCORE = "agentarcade:score";
/** Sent to the game when the agent finishes and the run has to stop. */
const FREEZE = "agentarcade:freeze";

export function createGeneratedGame(pack: Pack): GameFactory {
  return (): GameModule => {
    let frame: HTMLIFrameElement | null = null;
    let onMessage: ((event: MessageEvent) => void) | null = null;
    let frozen = false;

    return {
      id: pack.id,
      label: pack.label,
      unit: pack.unit,
      lowerIsBetter: pack.lowerIsBetter,

      mount(root: HTMLElement, api: GameApi): void {
        const el = document.createElement("iframe");
        // No allow-same-origin: that single omission is what makes the frame
        // untrusted-safe. Adding it would hand model-written code this
        // document's origin.
        el.setAttribute("sandbox", "allow-scripts");
        el.setAttribute("title", pack.label);
        el.style.width = "100%";
        el.style.height = "100%";
        el.style.border = "0";
        el.style.display = "block";
        el.src = pack.url;
        frame = el;

        onMessage = (event: MessageEvent) => {
          // Any frame on the page can post here, so identity is checked
          // against this iframe's own window rather than the message's
          // contents. An opaque origin serialises to "null", which makes
          // event.origin useless for the same purpose.
          if (!frame || event.source !== frame.contentWindow) return;
          if (frozen) return;
          const data = event.data as { type?: unknown; value?: unknown } | null;
          if (!data || data.type !== SCORE) return;
          if (typeof data.value !== "number" || !Number.isFinite(data.value)) return;
          api.reportScore(data.value);
        };
        window.addEventListener("message", onMessage);

        root.appendChild(el);
      },

      freeze(): void {
        if (frozen) return;
        frozen = true;
        // Best-effort: a generated game that ignores this just keeps running
        // until destroy() removes it, which is no worse than a built-in game
        // with a bug in its own freeze().
        frame?.contentWindow?.postMessage({ type: FREEZE }, "*");
      },

      destroy(): void {
        if (onMessage) {
          window.removeEventListener("message", onMessage);
          onMessage = null;
        }
        frame?.remove();
        frame = null;
        frozen = false;
      },
    };
  };
}
