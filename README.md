# claudegame (Free)

A tray app that pops up a quick mini-game whenever your AI assistant starts
thinking — and dismisses itself the moment it's done.

**This repo is the free edition** — [Download latest release](https://github.com/SirjanSingh/claudegame/releases/latest) (Windows `.exe`, macOS `.dmg`)

Inspired by [microtaskrr](https://github.com/whoisyurii/microtaskrr), with two
differences: cross-platform (Windows first), and you never press Esc to get back
to work. The `Stop` hook closes the game and returns focus to your editor.

<p align="center">
  <img src="docs/screenshots/scorecard.png" alt="Score card when the agent finishes — auto-hides after 1.5s" width="380">
</p>
<p align="center"><em>Agent finishes → score card → window hides. No keypress.</em></p>

---

## Install on Windows (recommended)

**Requirements:** Windows 10/11, [WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/) (pre-installed on most machines).

### 1. Download and install

1. Open **[Releases](https://github.com/SirjanSingh/claudegame/releases/latest)**
2. Download **`claudegame_1.0.1_x64-setup.exe`**
3. Run the installer — claudegame appears in the **system tray** (hidden icon area)

### 2. Start at login (recommended)

Right-click the tray icon → **Start at login**.

Hooks do nothing if the app is not running. Auto-start prevents silent misses.

### 3. Wire up your editor

Pick **one** (or both):

| Editor | Setup |
|--------|--------|
| **Cursor** | [Cursor hooks](#cursor) — agent chat in the IDE |
| **Claude Code** | Tray → **Enable Claude Code hooks** — terminal CLI |

### 4. Test it

**Tray menu → Play a game now** — a random free game should pop up.

Or from PowerShell (app must be running):

```powershell
$u = "http://127.0.0.1:47913/event"
irm $u -Method Post -ContentType "application/json" -Body '{"session_id":"test","hook_event_name":"UserPromptSubmit"}'
Start-Sleep -Seconds 3
irm $u -Method Post -ContentType "application/json" -Body '{"session_id":"test","hook_event_name":"Stop"}'
```

You should see a game, then a score card, then the window disappears.

---

## Install on macOS

**Requirements:** macOS 11+ (Apple Silicon or Intel), Node.js on PATH for Cursor hooks.

### 1. Download and install

1. Open **[Releases](https://github.com/SirjanSingh/claudegame/releases/latest)**
2. Download the `.dmg` for your Mac:
   - **Apple Silicon (M1/M2/M3):** `*_aarch64.dmg`
   - **Intel Mac:** `*_x64.dmg`
3. Open the `.dmg` and drag **claudegame** into **Applications**
4. Launch from Applications — the app lives in the **menu bar** (no Dock icon)

**Gatekeeper (unsigned build):** CI builds are not Apple-notarized yet. If macOS says the app is damaged or from an unidentified developer, either **right-click → Open** the first time, or run:

```bash
xattr -cr /Applications/claudegame.app
```

### 2. Start at login

Menu bar icon → **Start at login**.

### 3. Wire up your editor

Same as Windows — [Cursor hooks](#cursor) or tray → **Enable Claude Code hooks**.

### 4. Test it

Menu bar → **Play a game now**, or from Terminal (app must be running):

```bash
curl -s -X POST http://127.0.0.1:47913/event \
  -H 'Content-Type: application/json' \
  -d '{"session_id":"test","hook_event_name":"UserPromptSubmit"}'
sleep 3
curl -s -X POST http://127.0.0.1:47913/event \
  -H 'Content-Type: application/json' \
  -d '{"session_id":"test","hook_event_name":"Stop"}'
```

---

## Cursor

Claude Code hooks in `~/.claude/settings.json` **do not** fire from Cursor's agent.
Use the bundled bridge script:

```powershell
cd path\to\claudegame   # clone or extract the repo
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.cursor\hooks"
Copy-Item scripts\cursor-hook.js "$env:USERPROFILE\.cursor\hooks\claudegame.js"
```

Create `%USERPROFILE%\.cursor\hooks.json` (replace `YOUR_USER` with your Windows username):

```json
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\claudegame.js\" UserPromptSubmit" }],
    "preCompact":       [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\claudegame.js\" PreCompact" }],
    "stop":             [{ "command": "node \"C:\\Users\\YOUR_USER\\.cursor\\hooks\\claudegame.js\" Stop" }]
  }
}
```

**Restart Cursor.** Send an agent message — a game pops while it thinks, hides when done.

Requires **Node.js** on PATH (for the hook script). The tray app must be running.

---

## Claude Code

1. Launch claudegame (tray icon visible)
2. Tray → **Enable Claude Code hooks**
3. Use Claude Code in a terminal — a game appears on each prompt

Hooks merge into `~/.claude/settings.json` with a backup at
`~/.claude/settings.json.claudegame-backup`. **Disable Claude Code hooks** removes
only what claudegame added.

Manual wiring:

```jsonc
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }],
    "PreCompact":       [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }],
    "Stop":             [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }]
  }
}
```

---

## Games

**Four free**, **four Pro** (Pro gated until license — see [Pricing](#pricing)).
Random picker draws from free games; never the same twice in a row.

| Game | Tier | You | Scored on |
|---|---|---|---|
| **Snake** | Free | steer with arrows/WASD | apples per minute |
| **Typing** | Free | type the words | words per minute |
| **Aim** | Free | click the targets | targets per minute |
| **Reaction** | Free | wait for green, click | best reaction, ms |
| **Gradient** | Pro | swap tiles to restore a colour field | % sorted |
| **Metronome** | Pro | tap space on the beat | mean timing error, ms |
| **Untangle** | Pro | drag nodes until no lines cross | % of crossings removed |
| **Boulder** | Pro | jump over code blocks, outrun the sunburst | metres per minute |

### Free tier

<table>
  <tr>
    <td align="center" width="25%"><img src="docs/screenshots/snake.png" alt="Snake" width="200"><br><sub><b>Snake</b></sub></td>
    <td align="center" width="25%"><img src="docs/screenshots/typing.png" alt="Typing" width="200"><br><sub><b>Typing</b></sub></td>
    <td align="center" width="25%"><img src="docs/screenshots/aim.png" alt="Aim" width="200"><br><sub><b>Aim</b></sub></td>
    <td align="center" width="25%"><img src="docs/screenshots/reaction.png" alt="Reaction" width="200"><br><sub><b>Reaction</b></sub></td>
  </tr>
</table>

### Pro tier (gated)

<table>
  <tr>
    <td align="center" width="33%"><img src="docs/screenshots/gradient.png" alt="Gradient" width="200"><br><sub><b>Gradient</b> · Pro</sub></td>
    <td align="center" width="33%"><img src="docs/screenshots/metronome.png" alt="Metronome" width="200"><br><sub><b>Metronome</b> · Pro</sub></td>
    <td align="center" width="33%"><img src="docs/screenshots/untangle.png" alt="Untangle" width="200"><br><sub><b>Untangle</b> · Pro</sub></td>
  </tr>
</table>

### Stats

Tray → **Show stats** — per-game bests, sparklines, recent runs. All local.

<p align="center">
  <img src="docs/screenshots/stats.png" alt="Stats screen" width="380">
</p>

Two scoring rules (runs can end at any moment):

- **Score exists at every instant** — no "solved in N moves" games.
- **Rates and bests, not running totals** — a PB is skill, not wait duration.

---

## Tray menu

- **Play a game now** — test without sending a prompt
- **Show stats** — local bests and trends (Esc to close)
- **Pause** — 15 min / 1 hour / until resume
- **Enable / Disable Claude Code hooks**
- **Start at login**
- **Open stats file** — `%APPDATA%\com.sirjan.claudegame\stats.json`

---

## Build from source

For contributors or macOS/Linux (Windows installer is the tested path):

**Requirements:** Node 18+, Rust 1.82+, Tauri v2 prerequisites ([Windows](https://v2.tauri.app/start/prerequisites/): MSVC + WebView2).

```bash
git clone https://github.com/SirjanSingh/claudegame.git
cd claudegame
npm install
npm run tauri dev      # development
npm run tauri build    # installer → src-tauri/target/release/bundle/
```

On Windows with `CARGO_TARGET_DIR=D:\rust\target`, the NSIS installer lands at
`D:\rust\target\release\bundle\nsis\claudegame_1.0.1_x64-setup.exe`.

**Releases:** Push a `v*` tag (or run the **Release** workflow manually) to build
Windows and macOS installers on GitHub Actions — no Mac hardware required.

---

## Repo layout

```
src/games/free/          ← Free tier (always playable)
src/games/pro/           ← Pro tier (runtime-gated; future private repo)
scripts/cursor-hook.js   ← Cursor IDE bridge
```

| Edition | Repo | Games |
|---------|------|-------|
| **Free** | This repo (public) | 4 in `src/games/free/` |
| **Pro** | Private repo (future) | Pro folder + sync, multi-tool, extras |

---

## How it works

```
Editor hook ──HTTP POST──▶ 127.0.0.1:47913/event  (inside the tray app)
                                   │
                            SessionTracker
                            · 2s debounce  · session ref-count  · no-repeat picker
                                   │
                          show / freeze / hide  ──▶  game window
```

| Hook event | What happens |
|---|---|
| Prompt submitted | show a random free game |
| PreCompact | show a random free game |
| Stop | freeze, score card 1.5s, hide, return focus |

---

## Stats & privacy

- **Local JSON only** — no accounts, no telemetry, no backend
- **Windows path:** `%APPDATA%\com.sirjan.claudegame\stats.json`
- Hook server binds **`127.0.0.1` only** — not reachable from the network

---

## Configuration

`CLAUDEGAME_PORT` overrides port 47913. Set before launch; hook URLs must match.

`CLAUDEGAME_PRO=1` unlocks Pro games for development/testing.

---

## Pricing

**Free** — Snake, Typing, Aim, Reaction + hooks + local stats.

**Pro** (coming soon) — remaining games + cloud sync, multi-tool hooks, themes.
Set `CLAUDEGAME_PRO=1` or use **Pro (dev)** in browser mode to preview Pro games.

---

## Development

```bash
npm run dev            # browser mode + dev bar (no Rust)
npm run build          # typecheck + bundle
npm run test:rust      # 54 unit tests
python tests/capture_screenshots.py   # regenerate docs/screenshots
```

Browser driver: `claudegame.show("snake")` then `claudegame.stop()`.

---

## Adding a game

**Free:** `src/games/free/<name>.ts` → `free/index.ts` → `FREE_GAMES` in `src-tauri/src/logic/tiers.rs`

**Pro:** `src/games/pro/<name>.ts` → `pro/index.ts` → `PRO_GAMES` in `tiers.rs`

---

## License

MIT — see [LICENSE](LICENSE).

## Platform status

| | Status |
|---|---|
| Windows | Primary. NSIS installer on Releases (local or CI). Focus returns to previous window on hide. |
| macOS | **CI builds** on every release tag (`aarch64` + `x64` `.dmg`). Unsigned — see install notes above. |
| Linux | Code in place; not shipped on Releases yet. |
