# 2026-07-27 — First real runtime launch, focus bug, and product direction

The session that took the Rust/Tauri half from "typechecks" to "runs, verified, and
packaged", then re-scoped the product from a Claude-Code-only tray app to a general
AI-coding-assistant companion.

## What actually happened

### The Rust half executed for the first time

Everything in `src-tauri/src/` had only ever been typechecked (`cargo check`). This session
ran it:

- **`npm run test:rust` → 47/47 passing.** First execution of the suite. Covers the
  settings.json surgery, picker, tracker state machine, hook classification, and stats
  persistence/migration.
- **`npm run tauri dev` → launched.** Hook server bound to `127.0.0.1:47913`,
  `/health` answered, tray built, window created hidden.
- **Full hook round-trip verified against the running app.** `POST UserPromptSubmit`
  popped the window with a game (Boulder) over the editor; `POST Stop` ran the freeze →
  score card ("CLAUDE'S DONE / 99 m/min") → auto-hide. Screenshotted both.
- **`stats.rs` proven at runtime.** `%APPDATA%\com.sirjan.claudegame\stats.json` accumulated
  4 runs across app restarts with the v2 schema intact — real disk persistence, not the
  browser localStorage fallback.
- **`npm run tauri build` → succeeded.** 6.6 MB release binary, 1.7 MB NSIS installer at
  `D:\rust\target\release\bundle\nsis\claudegame_0.1.0_x64-setup.exe`.

### Bug found and fixed: focus never returned after a cold start

**Symptom.** After the first show/hide cycle following app start, the foreground window
stayed on claudegame's own *hidden* window. Focus never went back to the terminal — the
app's entire selling point.

**Root cause.** Tauri leaves the hidden main window as the foreground window right after
startup. So on the first `show()`, `GetForegroundWindow()` returned *our own window*,
`remember()` stored it, and `restore()` dutifully handed focus back to ourselves.
Reproduced deterministically at cold start: `before: 0x40D32 claudegame vis=False`
→ `after: 0x40D32 claudegame vis=False`.

Worth recording how noisy this was to pin down: it only reproduces when no *other* window
holds focus at show time. Every test that launched a terminal first masked it completely
(3/3 and 4/4 clean cycles). Tests that look like they exonerate the code may just be
supplying the missing precondition.

**Fix** ([window.rs](../../src-tauri/src/window.rs)): `remember()` now ignores any window
belonging to our own process (`GetWindowThreadProcessId` vs `GetCurrentProcessId`), and
keeps the previously remembered target rather than overwriting it with an unusable one.
The decision is factored into a pure `next_previous()` so it is unit-testable without a
window — matching how `tracker.rs` keeps its rules testable. 4 new tests, suite now 51/51.

### Environment fixes

- The session shell had inherited a stale `CARGO_TARGET_DIR=G:\rust\target`. G: is exFAT,
  which silently taxes every rebuild (see the `rust-toolchain-layout` memory). User scope
  was already correct at `D:\rust\target`; all builds this session were forced to D:.
- A stray vite dev server from the previous session held port 5183 and blocked the first
  `tauri dev`. Killed.

## Product direction (the substantial part)

Four decisions came out of discussion. **None of this is built yet.**

### 1. A delay before the game appears — approved, not implemented

The existing 2s `SHOW_DEBOUNCE` is only a flicker guard; it still shows a game
*immediately*, so a 3-second answer would pop one. Unusable in practice.

**Design (approved):** the trigger becomes *elapsed time*, not the prompt event.
`on_show_event` records `pending_since` and returns `Nothing`; a game appears only when
`tick()` sees a session that has been thinking continuously for ≥ the delay. `Stop` during
the pending window cancels it silently.

- Default **10s**, tray-adjustable (5/10/30/60) via `CheckMenuItem`s, persisted.
- Repeated show events must **not** reset `pending_since` (set only when `None`) — this
  handles flicker bursts naturally.
- `main.rs` tick loop must drop from **60s → 500ms**; a 60s tick cannot fire a 10s delay.
- Persistence via a new small `prefs.rs` (`prefs.json` in app data dir), mirroring
  `stats.rs`. Deliberately *not* stored in `stats.json` — different concern, different
  lifetime.

### 2. Generalize beyond Claude Code

Goal: work with Codex, Cursor, Gemini CLI, and whatever ships next — not just Claude Code.

**Key finding: the engine is already vendor-neutral.** All 7 "Claude" mentions in
`tracker.rs` are *comments*; there is zero code coupling. The debounce, per-session
ref-counting, stale sweep and picker are all generic. Real coupling is only:

- `server.rs:42-43` — two lines mapping Claude's event names
- `logic/hooks.rs` — the `~/.claude/settings.json` surgery

The per-session ref-counting already models multi-tool correctly: Claude in one pane and
Codex in another are simply two active sessions, and the window stays up until the last
finishes.

**Plan:** widen `/event` to a tool-neutral payload, keeping Claude's shape for
back-compat:

```jsonc
{ "state": "busy", "source": "codex", "session": "abc" }   // or "idle"
```

`classify()` becomes a set of adapters. Consequence: anything able to run a shell command
on an event integrates with one line of curl, without needing a written adapter.

Three integration tiers:

| Tier | Mechanism | Quality |
|---|---|---|
| 1 | Native hooks per tool (Claude Code today; Codex `notify`; others) | Exact turn boundaries |
| 2 | Heuristic — watch known agent processes sustaining CPU | Approximate, zero setup |
| 3 | Manual — tray "Play a game now" (already built) | Always works |

Tier 2 only becomes viable *because of* the 10s delay — a raw CPU heuristic is far too
twitchy alone, but "has been chewing for 10 seconds" is a usable signal. The two features
reinforce each other, which is why the delay should land first.

**Open:** the actual hook APIs for Codex / Gemini CLI / Cursor were **not** verified. Do
not write adapters from assumption — check current docs first.

### 3. Estimate-as-hint, never as trigger

Idea raised: have the agent estimate its own runtime up front and tell the app.

Rejected as a *trigger*, kept as an *enrichment*, because model self-estimates of
wall-clock time are poorly calibrated (the model doesn't know tool-call count or how slow
the test suite is), depend on per-turn prompt compliance, and cost tokens on every prompt
to serve the rare long one. Elapsed time is free, deterministic and vendor-independent.

Kept as an optional field, absent-is-fine:

```jsonc
{ "state": "busy", "source": "claude", "session": "x", "expected": "long" }
```

`long` → show immediately and skip the wait; absent/`quick` → normal timer.
Use **coarse buckets** (`quick|medium|long`), not seconds — models judge relative
magnitude far better than absolute time, and asking for a number invents false precision.

**Better signal, no model cooperation needed:** Claude Code's `UserPromptSubmit` hook
payload carries the prompt text, so complexity can be classified *locally* (length,
keywords) for free and deterministically. Verify what each tool's payload actually
includes before relying on it.

Priority of signals: elapsed time (primary) > local prompt heuristic > model self-estimate.

### 4. Distribution: npm first, exe second

The friction objection is correct — people readily `npm install`, but "go to GitHub and
download an exe" loses most of them.

It **must** ship a native binary: the app paints an always-on-top window and calls
`GetForegroundWindow`/`SetForegroundWindow`. A Claude Code plugin or MCP server ships
*config*, and gets no process, tray icon, or window. Not expressible as a plugin.

**But npm is a fine channel for a native binary** — esbuild/swc/sharp/Playwright all do
it, and at 6.6 MB this qualifies easily.

```bash
npm install -g claudegame
claudegame setup
```

Use **`optionalDependencies`** with per-platform packages (`@scope/win32-x64`, etc.) and
`os`/`cpu` fields, *not* a `postinstall` download script — no network beyond npm, works
behind proxies, caches in CI, can't half-fail. It's what esbuild moved to after years of
postinstall pain.

**This also sidesteps most code signing.** Windows SmartScreen keys off Mark-of-the-Web
and macOS Gatekeeper off the quarantine attribute — both applied by *browsers*. Binaries
delivered by npm generally carry neither, which is why so many CLI tools ship unsigned via
npm/Homebrew. Signing still matters later (Defender / Smart App Control can flag unsigned
binaries), but it is no longer a blocker for shipping.

Caveats: Linux needs system `webkit2gtk` which npm can't supply — ship Linux as the
AppImage and treat npm as Windows + macOS. macOS/Linux have still never been built or run
on real hardware.

**Why no Claude Code plugin for the hook wiring:** a plugin's `hooks.json` is static and
would hardcode port 47913, breaking under `CLAUDEGAME_PORT` or a port conflict. The tray
toggle knows the live port, backs up settings.json, and uninstalls as an exact inverse.
A plugin would also invert the dependency — wiring hooks to an app that may not be
installed, so every prompt hits a dead endpoint.

### 5. Rename — decision pending

`claudegame` hard-codes one vendor into a product meant for all of them: binary name, npm
package, `com.sirjan.claudegame` identifier, and the stats path. Cheap now, expensive once
anyone installs. **Decide before npm packaging** — the package name is the one thing that
can't be changed later.

## Files touched

- `src-tauri/src/window.rs` — focus-return fix + `next_previous()` + 4 tests
- `src-tauri/Cargo.lock` — committed (this is an app, not a library)
- `docs/sessions/` — new; this note

## Verification

- `cargo test` — 51/51 pass
- `tauri dev` — launched, hook round-trip driven by real HTTP POSTs, screenshotted
- `tauri build` — release binary + NSIS installer produced
- Focus return confirmed across repeated cycles against a real terminal window
  (took focus on show, returned it on hide, every cycle)

## Open items, in suggested order

1. **Build the delay** (design approved above). Everything else leans on it.
2. **Install the hooks for real** — the user chose "install for real now"; deliberately
   deferred so the 10s delay lands first and the first real experience isn't the annoying
   one. `~/.claude/settings.json` already contains a full pixel-agents hook set on
   `UserPromptSubmit`/`Stop`; claudegame appends alongside it and backs up first.
3. **Generic `/event` contract + rename.**
4. **Research Codex / Gemini CLI / Cursor hook APIs** before writing any adapter.
5. **npm packaging** with per-platform optional deps + release workflow.
6. Untested: macOS/Linux builds; auto-update (`tauri-plugin-updater`) needs signing keys
   decided at build time.
7. Stale artifacts in `G:\rust\target` (~1.9 GB) can be deleted — target dir is on D: now.
