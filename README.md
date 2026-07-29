# claudegame (Free)

A tray app that pops up a quick mini-game whenever your AI assistant starts
thinking — and dismisses itself the moment it's done.

**This repo is the free edition** ([github.com/SirjanSingh/claudegame](https://github.com/SirjanSingh/claudegame)).
Pro games and future Pro features will ship from a separate private repo; Pro
modules in `src/games/pro/` are runtime-gated here.

Inspired by [microtaskrr](https://github.com/whoisyurii/microtaskrr), with two
differences: it is cross-platform (Windows first), and you never have to press
Esc to get back to work. The `Stop` hook closes the game for you.

<p align="center">
  <img src="docs/screenshots/scorecard.png" alt="The score card shown when Claude finishes, before the window hides itself" width="380">
</p>
<p align="center"><em>Claude finishes, you get your score, the window leaves. No keypress.</em></p>

Eight games — **four free**, **four Pro**. The random picker draws from the free
set unless Pro is unlocked; never the same game twice in a row.

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

<table>
  <tr>
    <td align="center" width="33%"><img src="docs/screenshots/snake.png" alt="Snake" width="230"><br><sub><b>Snake</b> — apples per minute</sub></td>
    <td align="center" width="33%"><img src="docs/screenshots/reaction.png" alt="Reaction" width="230"><br><sub><b>Reaction</b> — wait for green</sub></td>
    <td align="center" width="33%"><img src="docs/screenshots/typing.png" alt="Typing" width="230"><br><sub><b>Typing</b> — words per minute</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/gradient.png" alt="Gradient" width="230"><br><sub><b>Gradient</b> — restore the colour field</sub></td>
    <td align="center"><img src="docs/screenshots/metronome.png" alt="Metronome" width="230"><br><sub><b>Metronome</b> — tap on the beat</sub></td>
    <td align="center"><img src="docs/screenshots/aim.png" alt="Aim" width="230"><br><sub><b>Aim</b> — targets per minute</sub></td>
  </tr>
  <tr>
    <td align="center"><img src="docs/screenshots/untangle.png" alt="Untangle" width="230"><br><sub><b>Untangle</b> — pull the knot apart</sub></td>
    <td align="center"><img src="docs/screenshots/stats.png" alt="Stats" width="230"><br><sub><b>Stats</b> — bests and trends, all local</sub></td>
    <td></td>
  </tr>
</table>

Two rules every game follows, because runs are interrupted at an arbitrary
moment and scored on the spot:

- **The score is defined at every instant.** No "solved in N moves" games —
  frozen three seconds in, that number does not exist.
- **The score does not drift with run length.** Rates, percentages and
  best-single-event times, never running totals. Otherwise a personal best is
  just a record of the longest wait you have ever had.

---

## Repo layout

```
src/games/free/     ← Free tier (Snake, Typing, Aim, Reaction) — always playable
src/games/pro/      ← Pro tier (Gradient, Metronome, Untangle, Boulder) — gated
src/tiers.ts        ← Tier lists (keep in sync with src-tauri/src/logic/tiers.rs)
scripts/cursor-hook.js   ← Cursor IDE hook bridge
```

| Edition | Repo | Games |
|---------|------|-------|
| **Free** | This repo (public) | 4 games in `src/games/free/` |
| **Pro** | Private repo (future) | Pro folder + sync, multi-tool, extras |

---

## How it works

```
Claude Code ──HTTP POST──▶ 127.0.0.1:47913/event  (axum, inside the tray app)
                                   │
                            SessionTracker
                            · 2s debounce  · session ref-count  · no-repeat picker
                                   │
                          show / freeze / hide  ──▶  game window
```

Claude Code supports `http` hooks natively, so there is no shell script, no
bundled CLI, and nothing to put on `PATH`. The hook posts its own JSON payload
(`session_id`, `hook_event_name`) straight to the app.

| Hook event | What happens |
|---|---|
| `UserPromptSubmit` | show a random game |
| `PreCompact` | show a random game (compaction is a long wait) |
| `Stop` | freeze the run, show the score for 1.5s, hide, return focus |

Three details that matter in practice:

- **2-second debounce.** Claude Code's terminal-flicker bug can fire
  `UserPromptSubmit` several times in a row; only the first opens a game.
- **Session ref-counting.** With two Claude Code windows open, one finishing
  does not close the game the other is still keeping open.
- **Stale sweep.** A terminal killed without a `Stop` releases the window after
  10 minutes instead of pinning it open forever.

## Install

Requirements: Node 18+, Rust 1.82+, and the platform's Tauri v2 prerequisites
(on Windows: MSVC build tools + WebView2 runtime).

```bash
npm install
npm run tauri dev      # development
npm run tauri build    # installer in src-tauri/target/release/bundle
```

Then, from the tray menu, choose **Enable Claude Code hooks**. That merges three
entries into `~/.claude/settings.json`, keeping a one-time copy of your original
at `~/.claude/settings.json.claudegame-backup`. **Disable Claude Code hooks**
removes exactly what it added and leaves everything else untouched.

### Cursor hooks

Claude Code hooks do **not** fire from Cursor's agent chat. Install the bundled
bridge script:

```powershell
# Copy hook script
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.cursor\hooks"
Copy-Item scripts\cursor-hook.js "$env:USERPROFILE\.cursor\hooks\claudegame.js"

# Create ~/.cursor/hooks.json (adjust path if your home dir differs)
@'
{
  "version": 1,
  "hooks": {
    "beforeSubmitPrompt": [{ "command": "node \"C:\\Users\\YOU\\.cursor\\hooks\\claudegame.js\" UserPromptSubmit" }],
    "preCompact":       [{ "command": "node \"C:\\Users\\YOU\\.cursor\\hooks\\claudegame.js\" PreCompact" }],
    "stop":             [{ "command": "node \"C:\\Users\\YOU\\.cursor\\hooks\\claudegame.js\" Stop" }]
  }
}
'@ | Set-Content "$env:USERPROFILE\.cursor\hooks.json"
```

Restart Cursor after installing. The tray app must be running.

To wire it up by hand instead:

```jsonc
{
  "hooks": {
    "UserPromptSubmit": [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }],
    "PreCompact":       [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }],
    "Stop":             [{ "hooks": [{ "type": "http", "url": "http://127.0.0.1:47913/event", "timeout": 2 }] }]
  }
}
```

## Tray menu

- **Play a game now** — ignores both the pause state and the debounce.
- **Show stats** — bests, trends and totals.
- **Pause** — 15 minutes, 1 hour, or until you resume. `Stop` still hides an
  open game while paused.
- **Enable / Disable Claude Code hooks**
- **Start at login** — worth turning on: hooks that fire while the app isn't
  running do nothing at all.
- **Open stats file**

## Stats

**Show stats** in the tray menu opens a screen with per-game bests, a trend
sparkline of recent runs, and how much waiting you have filled. Esc closes it.

Local JSON, no accounts and no backend. Written atomically (temp file + rename)
so a crash mid-write cannot truncate it; an unreadable file is preserved as
`stats.json.bak` rather than overwritten.

The file is schema v2, shaped so a sync layer could be added later without a
rewrite: every run carries a stable id and timestamp, and the install has a
random `deviceId` that is not derived from anything about you or the machine. A
v1 file upgrades in place and keeps its bests. Nothing is uploaded anywhere —
there is no server to upload to.

| Platform | Location |
|---|---|
| Windows | `%APPDATA%\com.sirjan.claudegame\stats.json` |
| macOS | `~/Library/Application Support/com.sirjan.claudegame/stats.json` |
| Linux | `~/.config/com.sirjan.claudegame/stats.json` |

## Configuration

`CLAUDEGAME_PORT` overrides the default port (47913) if something else has it.
Set it before launching the app, and use the same port in the hook URLs — the
tray installer writes whichever port the running app is using.

The server binds `127.0.0.1` only. It is never reachable from the network.

## Adding a game

**Free game:** write `src/games/free/<name>.ts`, register in `src/games/free/index.ts`,
add the id to `FREE_GAMES` in `src-tauri/src/logic/tiers.rs`.

**Pro game:** write `src/games/pro/<name>.ts`, register in `src/games/pro/index.ts`,
add the id to `PRO_GAMES` in `src-tauri/src/logic/tiers.rs`.

Each module implements `GameModule` from `src/game.ts` (`mount` / `freeze` /
`destroy`, and call `api.reportScore`).

## Development

```bash
npm run dev            # games in a plain browser, no Rust needed
npm run build          # typecheck + production bundle
npm run test:rust      # tracker, picker, hooks and stats unit tests
python tests/verify_games.py       # browser checks, against npm run dev
python tests/verify_new_games.py
python tests/verify_stats.py
python tests/capture_screenshots.py  # regenerate docs/screenshots
```

The Python suites need `playwright` (`pip install playwright && playwright
install chromium`) and a dev server already running. They drive the real page
and fire the same events the hooks do, which is how the frontend gets proven
independently of the Rust half.

Outside Tauri the page grows a control bar along the bottom — a game dropdown
plus Show / Claude done / Hide / Stats — which fires exactly the events the
hooks do. Stats live in `localStorage` there instead of `stats.json`.

The same driver is on `window.claudegame` for scripting:

```js
claudegame.show("snake");   // what UserPromptSubmit does
claudegame.stop();          // what Stop does
```

Drive the running app directly over its own endpoint:

```powershell
$u = "http://127.0.0.1:47913/event"
irm $u -Method Post -ContentType application/json -Body '{"session_id":"a","hook_event_name":"UserPromptSubmit"}'
irm $u -Method Post -ContentType application/json -Body '{"session_id":"a","hook_event_name":"Stop"}'
```

## Pricing

**Free tier** — Snake, Typing, Aim, and Reaction. Claude Code hooks, local stats,
tray controls. No account required.

**Pro** (coming soon) — Gradient, Metronome, Untangle, and Boulder, plus cloud
stats sync, multi-tool hooks (Codex, Cursor, Gemini CLI), themes, and finer tray
controls (show delay, schedules).

Pro is not sold yet. To test Pro games during development, set
`CLAUDEGAME_PRO=1` before launching the app, or use the **Pro (dev)** toggle in
browser mode (`npm run dev`).

## Security & privacy

- The hook server binds **`127.0.0.1` only** — not reachable from the network.
- **Enable Claude Code hooks** merges three entries into `~/.claude/settings.json`
  and keeps a one-time backup at `~/.claude/settings.json.claudegame-backup`.
  **Disable** removes exactly what it added.
- Stats are **local JSON** — no accounts, no telemetry, no backend in v0.1.0.

## License

MIT — see [LICENSE](LICENSE).

## Platform status

| | Status |
|---|---|
| Windows | Primary target. Focus returns to the terminal on hide via `GetForegroundWindow`/`SetForegroundWindow`. |
| macOS | Code is in place (`Accessory` activation policy, no Dock icon), not yet built or run on real hardware. |
| Linux | Code is in place; focus restore is left to the window manager, which may or may not oblige. |
