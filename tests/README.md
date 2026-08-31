# Manual smoke tests

These are **manual** Playwright scripts, not an automated suite. They are
deliberately not wired into CI: they need a Python toolchain, a browser
download, and a dev server already running, which is a lot of flakiness to buy
for what they catch. CI runs `cargo test`, `tsc` and the game-id check instead.

Use these when you have changed a game, the score card, the tab strip, or the
stats screen — the parts unit tests do not reach.

## Setup

```bash
pip install -r tests/requirements.txt
playwright install chromium
```

## Running

The dev server must already be running in another terminal:

```bash
npm run dev            # serves on http://localhost:5183
```

Then:

```bash
python tests/verify_games.py         # every game mounts, scores and tears down
python tests/verify_new_games.py     # the more recently added games
python tests/verify_stats.py         # stats screen and persistence
python tests/capture_screenshots.py  # regenerates docs/screenshots/
```

Point them somewhere else with `AGENTARCADE_URL` if the dev server is not on
5183.

Each script prints `PASS`/`FAIL` per check and exits non-zero if anything
failed, so they compose fine in a shell loop.

## What they drive

The browser build exposes the same event sequence Rust emits, so these scripts
exercise the real frontend code path:

```js
agentarcade.show("snake")   // → game:show
agentarcade.stop()          // → game:freeze, score card, hide
```

They do **not** cover the Rust half: the delay, the tracker, hook installation,
the tray, or focus return. Those are unit-tested in `src-tauri/src/`, and the
end-to-end behaviour needs the checklist in the README.
