# Generated games

Status: approved design, not yet planned
Date: 2026-09-06

## Problem

Every game in agentarcade is a hand-written TypeScript module in `src/games/`,
registered in `src/registry.ts`, with its id repeated in
`src-tauri/src/logic/games.rs`. There are eight of them. Adding a ninth means a
release.

We want a user to describe a game they'd like to play and get it, without
waiting for us to build it.

A second want, deferred to its own spec: a game themed on what that user has
actually been working on, derived from their agent session history.

## Constraints that shaped this

1. **No network.** "There is no HTTP client in the binary" is the top of the
   README and the reason the app is safe to install. Nothing here may add one.
2. **The delay is the product.** A game appears because the user is already
   waiting ten seconds. Generation takes 30-60s, so generation can never sit in
   the show path.
3. **Model-written code runs in a Tauri webview** that has the
   `@tauri-apps/api` bridge, the settings writer and `stats.json` in reach.
   Generated code must be isolated, not trusted.

## Decisions

**The generator is the user's own coding agent.** agentarcade already talks to
Claude Code over hooks. A skill shipped with the repo asks the agent the user is
already paying for to write the game. No API key, no backend, no per-generation
cost to us, and the binary gains no HTTP client. The audience for this app is
exactly "people running a coding agent", so the requirement excludes nobody.

Rejected: a bundled key (needs a backend, we pay per game, breaks the network
promise), bring-your-own key (still an HTTP client in the binary), and a
templated offline generator (offline and instant, but it is reskinning, not
"the game you described").

**Generated games run in a sandboxed iframe.** One self-contained HTML file in
`<iframe sandbox="allow-scripts">` with no `allow-same-origin`. Score returns
over `postMessage`. A thin `GameModule` wraps the iframe, so the shell, score
card, freeze-on-Stop and stats work unchanged.

Rejected: importing the generated module directly, which puts model-written
code in the same context as the Tauri bridge; and a JSON game DSL, which
removes the sandbox problem but caps every game at what the DSL already
expresses and leaves us maintaining a game language.

**The entry point is a slash command.** `/agentarcade:make <idea>` in the
terminal, where the agent already lives. No new UI in the app, and generation
is visible and interruptible in the place the user is already looking.

A second agent can generate while the first is busy, so a user may also ask
during a wait. What that does not buy is immediacy: a 30-60s generation is
longer than most waits, so the game arrives in a later wait. An in-window entry
point is therefore deferred, not impossible.

**Generated games are deliberately small.** One canvas, one mechanic, one
scoring rule. No menus, no levels, no assets, no sound. Roughly 150 lines. This
is a hard constraint in the contract, not a style note: it keeps generation
fast and cheap enough to run on a small model, keeps the sandboxed surface
small, and matches how the game is used - two minutes while something compiles.

**The pack format doubles as a contribution format.** A pack carries the prompt
that produced it and the same metadata a built-in game exposes, so a good pack
can later become a pull request against this repo without changing its shape.
That is a follow-on spec (see Later), but the format is fixed now so it does
not need redesigning then.

## Non-goals

- Editing or regenerating an existing pack.
- Pack versioning.
- A preview or gallery UI.
- **Importing packs from other users.** A shareable executable game pack is a
  malware distribution channel. Sharing goes through pull requests and human
  review, never through the app.
- A creativity score. A model grading the user's own idea is a number with no
  audience, and any number attached to a public ranking gets farmed. The reward
  for a good idea is that the game gets merged and ships to everyone.

## Architecture

### The pack

A directory in the existing app data dir, beside `prefs.json` and `stats.json`:

```
com.sirjan.agentarcade/games/<slug>/
  game.html    self-contained; no external references
  meta.json
```

`meta.json`:

```json
{
  "id": "semicolon-dodge",
  "label": "Semicolon Dodge",
  "unit": "pts",
  "lowerIsBetter": false,
  "prompt": "dodge falling semicolons",
  "createdAt": "2026-09-06T11:02:44Z"
}
```

`id`, `label`, `unit` and `lowerIsBetter` mirror `GameModule` exactly. `prompt`
is provenance: it is what makes a pack reviewable as a contribution later.

This directory is the entire interface between the agent and the app. The agent
writes files; the app reads them. Neither knows anything else about the other.

### Rust

A new `src-tauri/src/packs.rs` scans the games directory, validates each
`meta.json`, and returns the valid ids. `logic/games.rs` merges those into the
rotation pool alongside the built-in ids. `scripts/check-game-ids.mjs` narrows
its parity check to built-ins, since generated ids exist only at runtime.

Validation rejects, silently and per-pack:

- a slug that is not `[a-z0-9-]{1,40}` (closes path traversal),
- `meta.json` that is not valid JSON or is missing a required field,
- an `id` that collides with a built-in,
- `game.html` larger than 256 KB or absent.

A rejected pack is skipped and never enters rotation. One bad pack never stops
the others loading.

### Frontend

A new `src/games/generated.ts` exports a factory taking a slug and returning a
`GameModule`:

- `mount()` fetches the pack's HTML through a Tauri command exposed by
  `packs.rs` (the webview has no filesystem access of its own), prepends the
  CSP meta described below, and sets it as the `srcdoc` of an
  `<iframe sandbox="allow-scripts">`. `srcdoc` rather than a file URL, so the
  frame has no origin to inherit. It listens for
  `{ type: "agentarcade:score", value: number }` from that frame and forwards
  it to `api.reportScore`.
- `freeze()` posts `{ type: "agentarcade:freeze" }` into the frame and stops
  accepting further scores. Idempotent.
- `destroy()` removes the iframe and the listener. Idempotent.

`registry.ts` becomes built-ins plus packs discovered at startup. Nothing
downstream - picker, tab bar, score card, stats - learns that a game is
generated, beyond a small badge in the picker.

### The contract given to the agent

Shipped as a reference file next to the skill, so the model has one place to
read and we have one place to change:

- One HTML file. No external scripts, styles, fonts or images. No network calls.
- No `localStorage` or `sessionStorage`. The sandbox has an opaque origin and
  those throw.
- Fill the viewport; the window is small and resizable.
- Report score with
  `parent.postMessage({ type: 'agentarcade:score', value: n }, '*')`, as often
  as you like. The shell keeps the latest.
- On `message` with `type: 'agentarcade:freeze'`, stop the run and leave the
  last frame on screen.
- One mechanic, one scoring rule, about 150 lines. No menus, no start screen,
  no levels.

## Isolation

Three layers, because the file is model-written and lives on disk where
anything else can edit it:

1. `sandbox="allow-scripts"` with no `allow-same-origin`. Opaque origin, so no
   Tauri bridge, no parent DOM, no storage.
2. The app prepends
   `default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:`
   as a CSP meta before the pack's own HTML. A generated `fetch()` fails even
   if the model writes one, so "nothing leaves your machine" stays true.
3. Messages are matched against the current frame's `contentWindow`; anything
   from another source is dropped.

The validation rules above are the fourth layer, applied before any of this.

## Error handling

- Invalid pack: skipped at scan time, never rotated in.
- Pack mounts but never reports a score: recorded as a no-score run, which is
  the same path as quitting early today.
- Pack throws inside the iframe: contained by the frame. The shell still shows
  the score card when Stop arrives.
- Recovery: settings gains a list of generated games with a delete button.

## Testing

Rust: discovery of a valid pack; rejection of a bad slug, bad JSON, a missing
field, an oversized `game.html`, and a built-in id collision; one bad pack does
not hide a good one.

Frontend: freeze reaches the iframe; a message from a foreign frame is ignored;
a run with no reported score records correctly; `destroy()` twice is safe.

One hand-written pack is committed as a fixture so the suite never needs an
agent to run.

## Files touched

- new `src-tauri/src/packs.rs`
- `src-tauri/src/logic/games.rs`
- `src-tauri/src/main.rs` (wire the scan)
- new `src/games/generated.ts`
- `src/registry.ts`, `src/picker.ts`, `src/settings.ts`
- `scripts/check-game-ids.mjs`
- new skill directory with the command and the contract reference
- new test fixture pack
- README: a section on what generated games can and cannot do

## Later

**B - personalised games.** A second skill reads the user's session history,
summarises what they have been working on, and calls the same generator. No app
changes at all: the app never reads a transcript, because the agent already
can.

**C - contribute a game.** A skill that turns a pack into a pull request
against this repo using `gh`. GitHub supplies everything a sharing feature
needs - hosting, identity, moderation by review, reactions in place of likes,
distribution in the next release - and the binary still gains no network code,
because the agent opens the PR, not agentarcade. Blocked on the repo being
public.

**D - public leaderboard with accounts and likes. Rejected for now.** It needs
a backend, a database, accounts and a privacy policy, and it puts an HTTP
client in the binary, which deletes the README section that makes this app safe
to install. It also means hosting and serving user-written executable
JavaScript to strangers, which requires an abuse queue from day one. C delivers
most of the same payoff at none of that cost.
