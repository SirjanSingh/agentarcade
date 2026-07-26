# claudegame

A tray app that pops up a quick mini-game whenever Claude Code starts thinking —
and dismisses itself the moment Claude is done.

Inspired by [microtaskrr](https://github.com/whoisyurii/microtaskrr), with two
differences: it is cross-platform (Windows first), and you never have to press
Esc to get back to work. The `Stop` hook closes the game for you.

Games: **Snake**, **Reaction**, **Typing**. A random one each time, never the
same twice in a row.

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
- **Pause** — 15 minutes, 1 hour, or until you resume. `Stop` still hides an
  open game while paused.
- **Enable / Disable Claude Code hooks**
- **Start at login** — worth turning on: hooks that fire while the app isn't
  running do nothing at all.
- **Open stats file**

## Stats

Local JSON, no accounts and no backend. Written atomically (temp file + rename)
so a crash mid-write cannot truncate it; an unreadable file is preserved as
`stats.json.bak` rather than overwritten.

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

1. Write `src/games/<name>.ts` implementing `GameModule` from `src/game.ts`
   (`mount` / `freeze` / `destroy`, and call `api.reportScore`).
2. Add it to `REGISTRY` in `src/registry.ts`.
3. Add the same id to `GAMES` in `src-tauri/src/main.rs`.

## Development

```bash
npm run dev            # games in a plain browser, no Rust needed
npm run build          # typecheck + production bundle
npm run test:rust      # tracker, picker, hooks and stats unit tests
```

In browser mode a `window.claudegame` driver stands in for Rust, so you can
exercise the real event sequence without building the app:

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

## Platform status

| | Status |
|---|---|
| Windows | Primary target. Focus returns to the terminal on hide via `GetForegroundWindow`/`SetForegroundWindow`. |
| macOS | Code is in place (`Accessory` activation policy, no Dock icon), not yet built or run on real hardware. |
| Linux | Code is in place; focus restore is left to the window manager, which may or may not oblige. |
