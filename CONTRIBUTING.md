# Contributing

The most useful contribution is **a new game**. That path is deliberately short.

## Where to start

- Issues labelled [`good first issue`](https://github.com/SirjanSingh/agentarcade/labels/good%20first%20issue)
  are small and self-contained, and most can be done without a Rust toolchain.
- Issues labelled [`new game`](https://github.com/SirjanSingh/agentarcade/labels/new%20game)
  are open game ideas. Comment that you're taking one so two people don't build
  the same thing.
- For anything bigger, open an issue first and say what you have in mind. A
  short conversation beats a large PR that goes in the wrong direction.

**Hacktoberfest:** PRs that close an issue or add a working game get the
`hacktoberfest-accepted` label once merged. Renamed variables, reworded docs
and whitespace churn get closed without review.

By contributing you agree your work is released under the [MIT license](LICENSE)
and that you'll follow the [code of conduct](CODE_OF_CONDUCT.md).

## Adding a game

A game is one self-contained module. It gets a container element and an API for
reporting its score, and it is told when to freeze and when to tear down. It
knows nothing about hooks, windows, or stats.

**1. Write `src/games/<id>.ts`** implementing `GameModule` from
[`src/game.ts`](src/game.ts):

```ts
export interface GameModule {
  readonly id: string;
  readonly label: string;
  /** Shown after the score, e.g. "wpm", "ms", "points". */
  readonly unit: string;
  /** True for times and other scores where lower wins. */
  readonly lowerIsBetter: boolean;
  mount(root: HTMLElement, api: GameApi): void;
  /** Stop accepting input; the run is over but the score card is not up yet. */
  freeze(): void;
  /** Release timers, listeners, animation frames. */
  destroy(): void;
}
```

`destroy()` matters more than it looks. The window is reused for every run, so
a leaked `requestAnimationFrame` or `setInterval` keeps burning battery for the
rest of the session.

**2. Add one line** to `REGISTRY` in [`src/registry.ts`](src/registry.ts).

**3. Add the same id** to `GAME_IDS` in
[`src-tauri/src/logic/games.rs`](src-tauri/src/logic/games.rs).

Rust picks which game to show, so it needs the id too. `npm run check:game-ids`
enforces that the two lists agree, and CI runs it.

### Trying it without a Rust toolchain

```bash
npm run dev
```

This serves the frontend in a plain browser with a dev bar that fires exactly
the events Rust would: pick your game from the dropdown, press **Show**, then
**Agent done**. You can also drive it from the console with
`agentarcade.show("your-id")` and `agentarcade.stop()`.

### What makes a good game here

- **Playable in ten seconds.** It exists to fill a wait, not to become the wait.
- **Interruptible.** It can be frozen mid-run at any moment, and that has to
  look deliberate rather than broken.
- **One number.** Scores are compared against a personal best, so a game needs
  exactly one, and `lowerIsBetter` has to be honest about which direction wins.
- **Keyboard or mouse, not both.** The user's hands are already on the keyboard.
- **No network, no assets.** Everything is inlined and offline. The binary makes
  no outbound calls and that is a property worth keeping.

## Before you open a PR

```bash
npm run build          # tsc --noEmit + vite build
npm run test:rust      # cargo test
npm run check:game-ids
cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings
```

CI runs all four. Clippy is `-D warnings`, so a warning is a failure.

## Changing behaviour rather than adding a game

All of the show/hide rules live in
[`src-tauri/src/logic/tracker.rs`](src-tauri/src/logic/tracker.rs), which has no
Tauri types in it and is fully unit-tested with no window on screen. If you are
changing when a game appears, that is the file, and the change should come with
a test that fails without it.

Two rules that are not up for negotiation:

- **The hook endpoint always answers quickly and never blocks a prompt.** A slow
  or failing hook shows up in the user's transcript. This app must never be the
  reason a prompt looks broken.
- **We never destroy a user's `settings.json`.** Writes are atomic, a malformed
  file is refused rather than replaced, and uninstall is an exact inverse of
  install including key order.

## Reporting a bug

Use the bug report form. It asks for your OS, which agent you use (Claude Code,
Codex, Cursor or Antigravity), and a screenshot of
tray → **Settings & status…** → **Status**. That panel answers most of the
questions a bug report otherwise takes three rounds to establish.

Security problems go through the private channel in [SECURITY.md](SECURITY.md),
not a public issue.
