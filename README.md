# agentarcade

[![CI](https://github.com/SirjanSingh/agentarcade/actions/workflows/ci.yml/badge.svg)](https://github.com/SirjanSingh/agentarcade/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Latest release](https://img.shields.io/github/v/release/SirjanSingh/agentarcade?label=download)](https://github.com/SirjanSingh/agentarcade/releases/latest)

Your coding agent is thinking. Play snake until it's done.

agentarcade is a tray app that opens a small game once your agent has been
working for a while, then closes itself the moment it finishes. You never press
a key to get back to work. The `Stop` hook hides the game and puts focus back in
your editor.

**[Download the latest release](https://github.com/SirjanSingh/agentarcade/releases/latest)**
for Windows (`.exe`) or macOS (`.dmg`).

<p align="center">
  <img src="docs/demo.gif" alt="A short prompt is answered and nothing happens. A longer one crosses ten seconds, a snake game appears, and it hides itself the moment the agent finishes." width="760">
</p>

<p align="center">
  <em>Want the longer version? There's a
  <a href="docs/tour.mp4">60-second tour</a> with all eight games, the settings
  and sound.</em>
</p>

---

## The delay is the whole point

Most prompts get answered in about three seconds, and a game that opens on every
one of those is unusable. So agentarcade waits until your agent has been
thinking continuously for **10 seconds** before it interrupts you. Anything
shorter and no window ever opens.

Set it to 5, 10, 30 or 60 seconds from the tray menu.

---

## What it does to your machine

Worth knowing up front, since this is a tray app that offers to edit your
editor's config file.

**No network.** There is no HTTP client in the binary. No accounts, no
telemetry, no backend. Nothing leaves your machine, because there's nothing in
there that could send it.

**One local server**, bound to `127.0.0.1:47913`. That's how your editor's hooks
reach it. It rejects any request whose `Host` header isn't that exact loopback
address, which closes off the DNS rebinding path a web page could otherwise use
to reach a server on your own machine.

**It writes to `~/.claude/settings.json`, but only when you ask it to.**
Clicking "Enable agent hooks" copies your file to
`settings.json.agentarcade-backup` first, then merges in three entries. The
write goes through a temp file and an atomic rename, so a crash halfway through
can't leave you with a truncated config. If your settings file isn't valid JSON,
agentarcade refuses to touch it and says so instead of replacing it. Turning
hooks off removes exactly the three entries it added and leaves everything else
alone, including your own hooks on the same events.

**Stats are one local JSON file**, at
`%APPDATA%\com.sirjan.agentarcade\stats.json` on Windows or
`~/Library/Application Support/` on macOS, next to `prefs.json`. The `deviceId`
in there is a random number. It isn't derived from your machine, your username,
or anything about you, and it never leaves the file.

---

## Install on Windows

Needs Windows 10 or 11 and the
[WebView2 runtime](https://developer.microsoft.com/en-us/microsoft-edge/webview2/),
which is already on most machines.

1. Download `agentarcade_*_x64-setup.exe` from
   [Releases](https://github.com/SirjanSingh/agentarcade/releases/latest)
2. Run it. A setup window opens on first launch and walks you through the two
   things that need doing.
3. After that it lives in the system tray, in the hidden icon area.

Closed the setup window by accident? It's under tray, **Settings & status**.

## Install on macOS

Needs macOS 11 or later, Apple Silicon or Intel.

1. Download the `.dmg` from
   [Releases](https://github.com/SirjanSingh/agentarcade/releases/latest).
   `*_aarch64.dmg` for M-series, `*_x64.dmg` for Intel.
2. Drag **agentarcade** into **Applications** and launch it. It lives in the
   menu bar with no Dock icon.

These builds aren't Apple-notarized. If macOS tells you the app is damaged or
from an unidentified developer, either right-click and choose **Open** the first
time, or run:

```bash
xattr -cr /Applications/agentarcade.app
```

---

## Setup

Two things, both in the setup window and in the tray menu.

**1. Start at login.** Hooks do nothing while the app isn't running, and that's
the one way agentarcade quietly does nothing at all.

**2. Wire up your editor.**

### Claude Code

Tray, then **Enable agent hooks**. See
[What it does to your machine](#what-it-does-to-your-machine) for exactly what
that writes.

### Cursor

Claude Code hooks don't fire from Cursor's agent, so use the bundled bridge:

```powershell
cd path\to\agentarcade   # clone or extract the repo
New-Item -ItemType Directory -Force -Path "$env:USERPROFILE\.cursor\hooks"
Copy-Item scripts\cursor-hook.js "$env:USERPROFILE\.cursor\hooks\agentarcade.js"
```

Create `%USERPROFILE%\.cursor\hooks.json`, replacing `YOUR_USER`:

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

Restart Cursor. Needs Node.js on your PATH.

### Check it worked

Tray, then **Play a game now**. Or tray, **Settings & status**, **Status**,
which tells you whether the hook endpoint is listening, whether hooks are
installed, and when the last hook arrived.

---

## The games

All eight ship to everyone. There's no paid tier and no gated content.

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

Switch between them mid-game with `Tab` and `Shift+Tab`. Turn any of them off in
**Settings**, **Options**.

---

## How it works

```
Editor hook --HTTP POST--> 127.0.0.1:47913/event   (inside the tray app)
                                   |
                                Tracker
         . per-session elapsed-time delay   . session ref-count
         . stale-session sweep              . no-repeat picker
                                   |
                        show / freeze / hide  -->  game window
```

| Hook event | What happens |
|---|---|
| Prompt submitted | start this session's clock |
| PreCompact | same |
| *(delay elapses, still thinking)* | show a random game |
| Stop | freeze, score card, hide, return focus |

The decision to show a game lives in a 500ms tick loop, not in the hook handler.
That is what makes a session which stops before the delay elapses open nothing
at all.

---

## Updates

Tray, then **Check for updates**. If there's a newer release it downloads,
verifies and installs it, then restarts.

It only checks when you click. There's no background update poll, because a
tray app that quietly phones home would contradict the rest of this page. The
download is checked against a signing key compiled into the binary, so a
release that isn't signed with the matching private key is refused.

---

## Configuration

`AGENTARCADE_PORT` overrides port 47913. Set it before launch. If hooks were
already installed on the old port, the Status view offers to clean up the
entries left behind.

Everything else is in the tray menu and under **Settings**, **Options**, and
persists to `prefs.json`.

---

## Development

```bash
npm install
npm run dev            # browser mode plus a dev bar, no Rust toolchain needed
npm run build          # typecheck and bundle
npm run test:rust      # unit tests
npm run check:game-ids # the TS and Rust game lists must agree
npm run tauri:dev      # the real app, with the dev-server CSP
npm run tauri build    # installer, into src-tauri/target/release/bundle/
```

Browser driver: `agentarcade.show("snake")` then `agentarcade.stop()`.

`tests/*.py` are manual Playwright smoke scripts. See
[tests/README.md](tests/README.md).

To cut a release, push a `v*` tag or run the **Release** workflow by hand.
GitHub Actions builds the Windows and macOS installers, so you don't need a Mac.

---

## Adding a game

Three touch points, and CI fails if the last two disagree:

1. `src/games/<id>.ts` implementing `GameModule` (see [`src/game.ts`](src/game.ts))
2. One line in `REGISTRY` in [`src/registry.ts`](src/registry.ts)
3. One id in `GAME_IDS` in [`src-tauri/src/logic/games.rs`](src-tauri/src/logic/games.rs)

More detail in [CONTRIBUTING.md](CONTRIBUTING.md).

---

## Platform status

| | Status |
|---|---|
| **Windows** | Primary, and the one I test on. NSIS installer on Releases. Focus returns to the previous window on hide. |
| **macOS** | CI builds an unsigned `.dmg` on every release tag. Focus return relies on the Accessory activation policy and has never been checked on real hardware. Reports welcome. |
| **Linux** | The code is there and builds an AppImage, but it isn't shipped on Releases and is community-tested only. |

---

## Support

agentarcade is free and MIT-licensed, and it stays that way. If it's saved you
from doom-scrolling through a few agent waits, you can
[sponsor it on GitHub](https://github.com/sponsors/SirjanSingh) or
[buy me a coffee](https://buymeacoffee.com/sirjansingh).

## License

MIT. See [LICENSE](LICENSE).
