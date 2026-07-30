import { REGISTRY } from "./registry";
import { isProGame } from "./tiers";

export interface GameTabs {
  refresh(): Promise<void>;
  setActive(gameId: string | null): void;
}

/**
 * Tab strip for switching games while Claude is thinking. Hidden on the stats
 * screen; the active tab tracks whichever game is mounted in the stage.
 */
export function installGameTabs(
  container: HTMLElement,
  getPickableIds: () => Promise<readonly string[]>,
  onSelect: (gameId: string) => void,
): GameTabs {
  const tabs = document.createElement("nav");
  tabs.id = "game-tabs";
  tabs.setAttribute("role", "tablist");
  tabs.setAttribute("aria-label", "Games");
  container.appendChild(tabs);

  const buttons = new Map<string, HTMLButtonElement>();
  let activeId: string | null = null;

  function setActive(gameId: string | null): void {
    activeId = gameId;
    for (const [id, btn] of buttons) {
      const selected = id === gameId;
      btn.setAttribute("aria-selected", selected ? "true" : "false");
      btn.tabIndex = selected ? 0 : -1;
    }
  }

  async function refresh(): Promise<void> {
    const ids = await getPickableIds();
    tabs.replaceChildren();
    buttons.clear();

    for (const id of ids) {
      const game = REGISTRY[id]();
      const btn = document.createElement("button");
      btn.type = "button";
      btn.role = "tab";
      btn.dataset.gameId = id;
      btn.textContent = game.label;
      if (isProGame(id)) btn.classList.add("pro");
      btn.addEventListener("click", () => onSelect(id));
      tabs.appendChild(btn);
      buttons.set(id, btn);
    }

    if (activeId && buttons.has(activeId)) setActive(activeId);
    else if (ids.length > 0) setActive(ids[0]);
  }

  void refresh();
  return { refresh, setActive };
}
