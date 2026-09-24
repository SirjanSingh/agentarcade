/**
 * The setup and diagnostics window.
 *
 * This exists because every way this app can fail used to be silent. If the
 * port is taken, if hooks were never installed, if they point at a port we
 * moved off — the tray icon looked identical and games simply never appeared.
 * The Status view is the answer to "it's running but nothing happens".
 */

import { invoke, listen } from "./bridge";

type ViewName = "welcome" | "status" | "options" | "games";

interface ServerStatus {
  state: "starting" | "listening" | "failed";
  reason?: string;
}

interface LastHookEvent {
  kind: string;
  sessionId: string;
  at: string;
}

interface Diagnostics {
  server: ServerStatus;
  port: number;
  hookUrl: string;
  hooksInstalled: boolean;
  settingsReadable: boolean;
  settingsPath: string;
  orphanedHooks: string[];
  lastHookEvent: LastHookEvent | null;
  showDelaySecs: number;
  paused: boolean;
}

/** Mirrors Pack in src-tauri/src/packs.rs. */
interface Pack {
  id: string;
  label: string;
  unit: string;
  lowerIsBetter: boolean;
  prompt: string;
  createdAt: string;
  url: string;
}

/** Mirrors SkillStatus in src-tauri/src/maker.rs. */
type SkillStatus = "absent" | "installed" | "foreign";

/** Mirror the camelCase serde names of the Rust enums in prefs.rs. */
type ThinkingContent = "games" | "feeds";
type FeedSite = "tikTok" | "instagram" | "youTubeShorts";
type FeedTarget = "appWindows" | "defaultBrowser";

const FEED_SITES: { id: FeedSite; label: string }[] = [
  { id: "tikTok", label: "TikTok" },
  { id: "instagram", label: "Instagram Reels" },
  { id: "youTubeShorts", label: "YouTube Shorts" },
];

interface Prefs {
  thinkingContent: ThinkingContent;
  feedSites: FeedSite[];
  feedTarget: FeedTarget;
  showDelaySecs: number;
  scorecardDurationMs: number;
  disabledGames: string[];
  firstRunComplete: boolean;
  allGames: string[];
}

const panel = document.getElementById("panel") as HTMLElement;
const tabs = Array.from(
  document.querySelectorAll<HTMLButtonElement>("#views button"),
);

let view: ViewName = "status";
let refreshTimer: number | undefined;

function el<K extends keyof HTMLElementTagNameMap>(
  tag: K,
  attrs: Record<string, string> = {},
  ...children: (Node | string)[]
): HTMLElementTagNameMap[K] {
  const node = document.createElement(tag);
  for (const [k, v] of Object.entries(attrs)) {
    if (k === "class") node.className = v;
    else node.setAttribute(k, v);
  }
  node.append(...children);
  return node;
}

function row(key: string, ...value: (Node | string)[]): HTMLElement {
  return el("div", { class: "row" }, el("span", { class: "key" }, key), el("span", { class: "val" }, ...value));
}

function badge(ok: boolean, text: string): HTMLElement {
  return el("span", { class: `badge ${ok ? "ok" : "bad"}` }, text);
}

function setView(next: ViewName): void {
  view = next;
  for (const tab of tabs) {
    tab.setAttribute("aria-selected", tab.dataset.view === next ? "true" : "false");
  }
  void render();
}

async function render(): Promise<void> {
  window.clearTimeout(refreshTimer);
  panel.replaceChildren(el("p", { class: "muted" }, "Loading…"));
  try {
    if (view === "status") await renderStatus();
    else if (view === "welcome") await renderWelcome();
    else if (view === "games") await renderGames();
    else await renderOptions();
  } catch (err) {
    panel.replaceChildren(
      el("h1", {}, "Something went wrong"),
      el("p", { class: "muted" }, String(err)),
    );
  }
}

/* -- Status ----------------------------------------------------------- */

function serverBadge(server: ServerStatus, port: number): HTMLElement {
  if (server.state === "listening") return badge(true, `listening on ${port}`);
  if (server.state === "starting") return badge(true, "starting…");
  return badge(false, `failed — ${server.reason ?? "unknown"}`);
}

async function renderStatus(): Promise<void> {
  const d = await invoke<Diagnostics>("diagnostics_snapshot");

  const rows = el("div", { class: "rows" });
  rows.append(
    row("Hook endpoint", serverBadge(d.server, d.port)),
    row(
      "Agent hooks",
      d.settingsReadable
        ? badge(d.hooksInstalled, d.hooksInstalled ? "installed" : "not installed")
        : badge(false, "settings.json unreadable"),
    ),
    row("settings.json", el("code", {}, d.settingsPath)),
    row("Hook URL", el("code", {}, d.hookUrl)),
    row(
      "Last hook received",
      d.lastHookEvent
        ? `${d.lastHookEvent.kind} · ${new Date(d.lastHookEvent.at).toLocaleTimeString()}`
        : el("span", { class: "muted" }, "nothing yet"),
    ),
    row("Show a game after", `${d.showDelaySecs} seconds of thinking`),
    row("Paused", d.paused ? badge(false, "yes") : badge(true, "no")),
  );

  const parts: (Node | string)[] = [
    el("h1", {}, "Status"),
    el(
      "p",
      { class: "muted" },
      "Everything here has to be green for a game to appear while your agent thinks.",
    ),
    rows,
  ];

  if (d.server.state === "failed") {
    parts.push(
      el(
        "div",
        { class: "note" },
        `The hook endpoint could not start, so games will never appear. Usually another copy of agentarcade is already running, or something else holds port ${d.port}. Quit the other copy, or set AGENTARCADE_PORT to a free port and restart.`,
      ),
    );
  }

  if (!d.settingsReadable) {
    parts.push(
      el(
        "div",
        { class: "note" },
        "Your settings.json exists but is not valid JSON. agentarcade will not modify it in that state — fix the file, then come back.",
      ),
    );
  }

  const actions = el("div", { class: "actions" });
  if (d.settingsReadable) {
    const toggle = el(
      "button",
      { class: "action primary", type: "button" },
      d.hooksInstalled ? "Disable agent hooks" : "Enable agent hooks",
    );
    toggle.addEventListener("click", async () => {
      toggle.disabled = true;
      await invoke("set_hooks_installed", { installed: !d.hooksInstalled });
      await render();
    });
    actions.append(toggle);
  }

  const test = el("button", { class: "action", type: "button" }, "Play a game now");
  test.addEventListener("click", () => void invoke("play_now"));
  actions.append(test);
  parts.push(actions);

  if (d.orphanedHooks.length > 0) {
    const clean = el(
      "button",
      { class: "action", type: "button" },
      `Remove ${d.orphanedHooks.length} stale hook ${d.orphanedHooks.length === 1 ? "entry" : "entries"}`,
    );
    clean.addEventListener("click", async () => {
      clean.disabled = true;
      await invoke("remove_orphaned_hooks");
      await render();
    });
    parts.push(
      el("h2", {}, "stale entries"),
      el(
        "p",
        { class: "muted" },
        "These point at a port agentarcade is not listening on. Each one costs every prompt a two-second timeout.",
      ),
      el("div", { class: "rows" }, ...d.orphanedHooks.map((u) => row("orphaned", el("code", {}, u)))),
      el("div", { class: "actions" }, clean),
    );
  }

  panel.replaceChildren(...parts);
  // Cheap enough to keep live, and the whole point is watching it turn green.
  refreshTimer = window.setTimeout(() => void render(), 3000);
}

/* -- Welcome ---------------------------------------------------------- */

async function renderWelcome(): Promise<void> {
  const d = await invoke<Diagnostics>("diagnostics_snapshot");

  const hookStep = el("li", {});
  hookStep.append(
    d.hooksInstalled
      ? el("span", {}, "Agent hooks are installed. ", badge(true, "done"))
      : el(
          "span",
          {},
          "Let agentarcade know when your agent is thinking. This adds three entries to ",
          el("code", {}, "settings.json"),
          " and backs up the original first.",
        ),
  );
  if (!d.hooksInstalled && d.settingsReadable) {
    const enable = el("button", { class: "action primary", type: "button" }, "Enable agent hooks");
    enable.addEventListener("click", async () => {
      enable.disabled = true;
      await invoke("set_hooks_installed", { installed: true });
      await render();
    });
    hookStep.append(el("div", { class: "actions" }, enable));
  }

  const test = el("button", { class: "action", type: "button" }, "Play a game now");
  test.addEventListener("click", () => void invoke("play_now"));

  const done = el("button", { class: "action primary", type: "button" }, "Done");
  done.addEventListener("click", async () => {
    await invoke("complete_onboarding");
    setView("status");
  });

  panel.replaceChildren(
    el("h1", {}, "Welcome to agentarcade"),
    el(
      "p",
      {},
      "When your coding agent starts thinking, a small game appears. When it finishes, the game closes itself and hands focus back to your editor. You never press a key to get back to work.",
    ),
    el("h2", {}, "two things to set up"),
    el(
      "ol",
      { class: "steps" },
      hookStep,
      el(
        "li",
        {},
        el("span", {}, "Start agentarcade at login. Hooks do nothing while the app is not running, which is the one way this silently does nothing. Set it from the tray menu."),
      ),
    ),
    el("h2", {}, "check it works"),
    el("p", { class: "muted" }, "This opens a random game exactly as a hook would."),
    el("div", { class: "actions" }, test, done),
  );
}

/* -- Options ---------------------------------------------------------- */

async function renderOptions(): Promise<void> {
  const prefs = await invoke<Prefs>("get_prefs");

  const modes = el("div", { class: "choices" });
  const feedInputs: HTMLInputElement[] = [];
  const syncFeedInputs = (content: ThinkingContent): void => {
    for (const input of feedInputs) input.disabled = content !== "feeds";
  };
  for (const [content, label] of [
    ["games", "Games"],
    ["feeds", "Short videos"],
  ] as const) {
    const input = el("input", { type: "radio", name: "content" }) as HTMLInputElement;
    input.checked = prefs.thinkingContent === content;
    input.addEventListener("change", () => {
      syncFeedInputs(content);
      void invoke("set_thinking_content", { content });
    });
    modes.append(el("label", { class: "choice" }, input, label));
  }

  const sites = el("div", { class: "games" });
  for (const { id, label } of FEED_SITES) {
    const input = el("input", { type: "checkbox" }) as HTMLInputElement;
    input.checked = prefs.feedSites.includes(id);
    input.addEventListener("change", () =>
      void invoke("set_feed_enabled", { site: id, enabled: input.checked }),
    );
    feedInputs.push(input);
    sites.append(el("label", { class: "toggle" }, input, label));
  }

  const targets = el("div", { class: "targets" });
  for (const [target, label, help] of [
    [
      "appWindows",
      "App windows",
      "Each opens in its own window that hides and pauses when the agent is done. Sign in once; it stays signed in.",
    ],
    [
      "defaultBrowser",
      "My default browser",
      "Uses your browser and its logins. Opens new tabs each time; closing them is up to you.",
    ],
  ] as const) {
    const input = el("input", { type: "radio", name: "target" }) as HTMLInputElement;
    input.checked = prefs.feedTarget === target;
    input.addEventListener("change", () => void invoke("set_feed_target", { target }));
    feedInputs.push(input);
    targets.append(
      el(
        "label",
        { class: "target" },
        input,
        el("span", {}, el("strong", {}, label), el("span", { class: "muted" }, help)),
      ),
    );
  }
  syncFeedInputs(prefs.thinkingContent);

  const choices = el("div", { class: "choices" });
  for (const secs of [5, 10, 30, 60]) {
    const input = el("input", { type: "radio", name: "delay" }) as HTMLInputElement;
    input.checked = prefs.showDelaySecs === secs;
    input.addEventListener("change", () => void invoke("set_show_delay", { secs }));
    choices.append(el("label", { class: "choice" }, input, `${secs}s`));
  }

  const games = el("div", { class: "games" });
  for (const id of prefs.allGames) {
    const input = el("input", { type: "checkbox" }) as HTMLInputElement;
    input.checked = !prefs.disabledGames.includes(id);
    input.addEventListener("change", () =>
      void invoke("set_game_enabled", { gameId: id, enabled: input.checked }),
    );
    games.append(el("label", { class: "toggle" }, input, id));
  }

  panel.replaceChildren(
    el("h1", {}, "Options"),
    el("h2", {}, "while the agent thinks"),
    modes,
    el("h2", {}, "short videos to open"),
    el(
      "p",
      { class: "muted" },
      "Each ticked site opens side by side. These load from the internet; games never do.",
    ),
    sites,
    el("h2", {}, "open them in"),
    targets,
    el("h2", {}, "show it after"),
    el(
      "p",
      { class: "muted" },
      "How long your agent has to be thinking before a game is worth the interruption. A quick answer should never open one.",
    ),
    choices,
    el("h2", {}, "games in the rotation"),
    el("p", { class: "muted" }, "Turning all of them off is ignored — the rotation falls back to every game."),
    games,
  );
}

/* -- Games ------------------------------------------------------------ */

/**
 * Generated games: the ones the user's own agent wrote. The app cannot make
 * these itself — it has no model and no network — so this view is mostly about
 * getting the command installed and then managing what came out of it.
 */
async function renderGames(): Promise<void> {
  const [packs, skill] = await Promise.all([
    invoke<Pack[]>("list_packs"),
    invoke<SkillStatus>("maker_skill_status"),
  ]);

  const parts: (Node | string)[] = [
    el("h1", {}, "Games you made"),
    el("h2", {}, "the game-maker command"),
  ];

  if (skill === "foreign") {
    parts.push(
      el(
        "p",
        { class: "muted" },
        "A skill folder called agentarcade-make already exists and agentarcade did not write it, so it has been left alone. Rename or remove it yourself if you want the command installed here.",
      ),
    );
  } else {
    const installed = skill === "installed";
    const toggle = el(
      "button",
      { class: "action", type: "button" },
      installed ? "Remove the command" : "Install the command",
    );
    toggle.addEventListener("click", async () => {
      toggle.disabled = true;
      try {
        await invoke("set_maker_skill_installed", { installed: !installed });
      } catch (err) {
        parts.length = 0;
        panel.replaceChildren(el("p", { class: "muted" }, String(err)));
        return;
      }
      await render();
    });
    parts.push(
      el(
        "p",
        { class: "muted" },
        installed
          ? "Type /agentarcade:make and what you want to play in your agent's terminal. The game shows up here within ten seconds."
          : "This writes a skill into ~/.claude/skills/ so your coding agent can write games for you. agentarcade never generates anything itself and never goes online.",
      ),
      el("div", { class: "actions" }, toggle),
    );
  }

  parts.push(el("h2", {}, "installed games"));

  if (packs.length === 0) {
    parts.push(
      el(
        "p",
        { class: "muted" },
        "None yet. Games you generate appear here, and can be switched off in the rotation from Options like any other game.",
      ),
    );
  } else {
    const rows = el("div", { class: "rows" });
    for (const pack of packs) {
      const remove = el("button", { class: "action", type: "button" }, "Delete");
      remove.addEventListener("click", async () => {
        remove.disabled = true;
        await invoke("delete_pack", { id: pack.id });
        await render();
      });
      rows.append(
        row(pack.label, el("span", { class: "muted" }, pack.prompt), remove),
      );
    }
    parts.push(rows);
  }

  panel.replaceChildren(...parts);
}

/* -- Boot -------------------------------------------------------------- */

for (const tab of tabs) {
  tab.addEventListener("click", () => setView(tab.dataset.view as ViewName));
}

void listen<string>("settings:view", (next) => setView(next as ViewName));

const requested = new URLSearchParams(window.location.search).get("view");
const known: ViewName[] = ["welcome", "options", "games"];
setView(known.includes(requested as ViewName) ? (requested as ViewName) : "status");
