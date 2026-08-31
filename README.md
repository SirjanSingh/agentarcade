# agentarcade

A tray app that shows a quick mini-game once your coding agent has been thinking
for a while — and dismisses itself the moment it's done.

You never press a key to get back to work. The `Stop` hook closes the game and
returns focus to your editor.

**[Download the latest release](https://github.com/SirjanSingh/agentarcade/releases/latest)** —
Windows `.exe`, macOS `.dmg`

<p align="center">
  <img src="docs/screenshots/scorecard.png" alt="Score card when the agent finishes — auto-hides after 1.5s" width="380">
</p>
<p align="center"><em>Agent finishes → score card → window hides. No keypress.</em></p>

---

## Why the delay matters

A game that pops on *every* prompt is unusable — most answers take three
seconds. agentarcade waits until your agent has been thinking continuously for
**10 seconds** (configurable: 5 / 10 / 30 / 60) before it interrupts you. A
quick answer never opens a window at all.

---

## Install on Windows

**Requirements:** Windows 10/11, [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on most machines).

1. Open **[Releases](https://github.com/SirjanSingh/agentarcade/releases/latest)** and download `agentarcade_*_x64-setup.exe`
2. Run the installer — a **setup window** opens on first launch and walks you
   through the two things that need doing
3. agentarcade then lives in the **system tray** (hidden icon area)

If you ever close the setup window, it is back under tray → **Settings & status…**

## Install on macOS

**Requirements:** macOS 11+ (Apple Silicon or Intel).

1. Download the `.dmg` for your Mac from
   [Releases](https://github.com/SirjanSingh/agentarcade/releases/latest) —
   `*_aarch64.dmg` for M-series, `*_x64.dmg` for Intel
2. Drag **agentarcade** into **Applications** and launch it. It lives in the
   **menu bar**, with no Dock icon.

**Gatekeeper (unsigned build):** CI builds are not Apple-notarized. If macOS
says the app is damaged or from an unidentified developer, either
**right-click → Open** the first time, or run:

```bash
xattr -cr /Applications/agentarcade.app
```

---

## Setup

Two things, both in the setup window or the tray menu:

**1. Start at login.** Hooks do nothing while the app is not running — this is
the one way agentarcade silently does nothing.

**2. Wire up your editor.**

### Claude Code

Tray → **Enable agent hooks**. This merges three entries into
`~/.claude/settings.json` and copies the original to
`settings.json.agentarcade-backup` first. Disabling removes exactly what it
added and leaves everything else — including your own hooks on the same
events — untouched.

If your `settings.json` is not valid JSON, agentarcade refuses to touch it and
says so rather than replacing it.

### Cursor

Claude Code hooks do **not** fire from Cursor's agent. Use the bundled bridge:

```powershell
cd path\to\agentarcade   # clone or extract the repo
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.cursor\hooks"
Copy-Item scripts\cursor-hook.js "$env:USERPROFILE\.cursor\hooks\agentarcade.js"
```

Create `%USERPROFILE%\.cursor\hooks.json` (replace `YOUR_USER`):

```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\agentarcade.js\" UserPromptSubmit" }],
    "preCompact":       [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\agentarcade.js\" PreCompact" }],
    "stop":             [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\agentarcade.js\" Stop" }]
  }
}
```

**Restart Cursor.** Requires Node.js on PATH.

### Check it works

Tray → **Play a game now**, or tray → **Settings & status…** → **Status**,
which shows whether the hook endpoint is listening, whether hooks are
installed, and when the last hook arrived.

---

## The games

All eight ship to everyone. There is no paid tier and no gated content.

| | |
|---|---|
| **snake** | the classic, on a small grid |
| **typing** | words per minute against the clock |
| **aim** | click targets before they expire |
| **reaction** | how fast can you hit the flash |
| **gradient** | match the colour |
| **metronome** | keep the beat |
| **untangle** | pull the graph apart |
| **boulder** | endless runner, dodging syntax |

Switch between them mid-game with `Tab` / `Shift+Tab`. Turn any of them off in
**Settings → Options**.

---

## How it works

```
Editor hook ──HTTP POST──▶ 127.0.0.1:47913/event  (inside the tray app)
                                   │
                               Tracker
        · per-session elapsed-time delay  · session ref-count
        · stale-session sweep             · no-repeat picker
                                   │
                          show / freeze / hide  ──▶  game window
```

| Hook event | What happens |
|---|---|
| Prompt submitted | start this session's clock |
| PreCompact | same |
| *(delay elapses while still thinking)* | show a random game |
| Stop | freeze, score card, hide, return focus |

A session that stops before the delay elapses never opens a window.

---

## Privacy and security

- **Local JSON only** — no accounts, no telemetry, no backend, and no outbound
  network calls of any kind. There is no HTTP client in the binary.
- Stats live at `%APPDATA%\com.sirjan.agentarcade\stats.json`
  (`~/Library/Application Support/` on macOS), alongside `prefs.json`.
- The per-install `deviceId` is randomly generated. It is not derived from your
  machine, your username, or anything else about you, and it never leaves the
  file.
- The hook server binds **`127.0.0.1` only**, and rejects any request whose
  `Host` header is not our own loopback endpoint — which is what closes the DNS
  rebinding path from a web page.
- Writes to your `settings.json` go through a temp file and an atomic rename, so
  a crash mid-write cannot truncate it.

---

## Configuration

`AGENTARCADE_PORT` overrides port 47913. Set it before launch. If hooks were
already installed on the old port, the Status view offers to clean up the
entries left behind.

Everything else lives in the tray menu and **Settings → Options**, and persists
to `prefs.json`.

---

## Development

```bash
npm install
npm run dev            # browser mode + dev bar, no Rust toolchain needed
npm run build          # typecheck + bundle
npm run test:rust      # unit tests
npm run check:game-ids # the TS and Rust game lists must agree
npm run tauri:dev      # the real app, with the dev-server CSP
npm run tauri build    # installer → src-tauri/target/release/bundle/
```

Browser driver: `agentarcade.show("snake")` then `agentarcade.stop()`.

`tests/*.py` are manual Playwright smoke scripts — see [tests/README.md](tests/README.md).

**Releases:** push a `v*` tag (or run the **Release** workflow manually) to
build Windows and macOS installers on GitHub Actions. No Mac hardware required.

---

## Adding a game

Three touch points, and CI fails if the last two disagree:

1. `src/games/<id>.ts` implementing `GameModule` (see [`src/game.ts`](src/game.ts))
2. One line in `REGISTRY` in [`src/registry.ts`](src/registry.ts)
3. One id in `GAME_IDS` in [`src-tauri/src/logic/games.rs`](src-tauri/src/logic/games.rs)

See [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Platform status

| | Status |
|---|---|
| **Windows** | Primary and tested. NSIS installer on Releases. Focus returns to the previous window on hide. |
| **macOS** | CI builds an unsigned `.dmg` on every release tag. Focus return relies on the Accessory activation policy and has not been verified on real hardware — reports welcome. |
| **Linux** | Code is in place and builds an AppImage, but it is not shipped on Releases and is community-tested only. |

---

## Support

agentarcade is free and MIT-licensed, and will stay that way. If it has saved
you from doom-scrolling through a few agent waits, you can
[sponsor it on GitHub](https://github.com/sponsors/SirjanSingh) or
[buy me a coffee](https://buymeacoffee.com/sirjansingh).

## License

MIT — see [LICENSE](LICENSE).
