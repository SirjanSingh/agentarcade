import type { BrowserDriver } from "./bridge";
import { getStats } from "./bridge";
import { REGISTRY } from "./registry";
import { isProGame } from "./tiers";

/**
 * Dev-only control bar, shown when the page runs in a plain browser instead of
 * the Tauri window. It fires exactly the same events the hooks do, so testing a
 * game is a click rather than a line of console typing.
 *
 * Never rendered in the packaged app — installBrowserDriver returns null there.
 */
export function installDevBar(driver: BrowserDriver | null, onStats: () => void): void {
  if (!driver) return;

  const bar = document.createElement("div");
  bar.id = "devbar";

  const tag = document.createElement("span");
  tag.className = "devbar-tag";
  tag.textContent = "browser mode";

  const select = document.createElement("select");
  select.id = "devbar-game";
  select.title = "Which game to open";
  const random = new Option("random (no repeats)", "");
  select.add(random);
  for (const id of driver.games) {
    const game = REGISTRY[id]();
    const suffix = isProGame(id) ? " · Pro" : "";
    select.add(new Option(`${game.label}${suffix}`, id));
  }

  const show = button("Show", "Same as UserPromptSubmit", () => {
    driver.show(select.value || undefined);
  });
  const stop = button("Claude done", "Same as Stop — freeze, score card, hide", () => {
    driver.stop();
  });
  const hide = button("Hide", "Dismiss without a score card", () => {
    driver.hide();
  });
  const stats = button("Stats", "Same as the tray's Show stats", onStats);
  const reset = button("Reset stats", "Clear local bests", () => {
    localStorage.removeItem("claudegame.stats");
    void refreshBests();
  });

  const bests = document.createElement("span");
  bests.className = "devbar-bests";

  async function refreshBests(): Promise<void> {
    const stats = await getStats();
    const parts = driver!.games
      .map((id) => {
        const best = stats.games[id]?.best;
        return best === null || best === undefined ? null : `${id} ${Math.round(best)}`;
      })
      .filter((p): p is string => p !== null);
    bests.textContent = parts.length ? parts.join(" · ") : "no bests yet";
  }

  // Changing the dropdown starts that game straight away — the common case is
  // "let me look at this one", not "let me configure then press go".
  select.addEventListener("change", () => driver.show(select.value || undefined));

  const proToggle = button("Pro (dev)", "Toggle Pro tier in browser mode (localStorage)", () => {
    const on = localStorage.getItem("claudegame.pro") === "1";
    localStorage.setItem("claudegame.pro", on ? "0" : "1");
    proToggle.textContent = on ? "Pro (dev)" : "Pro ✓";
  });
  if (localStorage.getItem("claudegame.pro") === "1") {
    proToggle.textContent = "Pro ✓";
  }

  bar.append(tag, select, show, stop, hide, stats, reset, proToggle, bests);
  document.body.appendChild(bar);
  document.body.classList.add("has-devbar");

  void refreshBests();
  // The score card writes stats on its way out; pick that up a beat later.
  window.addEventListener("claudegame:recorded", () => void refreshBests());
}

function button(label: string, title: string, onClick: () => void): HTMLButtonElement {
  const el = document.createElement("button");
  el.type = "button";
  el.textContent = label;
  el.title = title;
  el.addEventListener("click", onClick);
  return el;
}
