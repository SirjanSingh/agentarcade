---
name: agentarcade-make
description: Use when the user asks for a new agentarcade mini-game - "make me a game where...", "/agentarcade:make ...", "add a game to agentarcade", or any request for a custom game to play while their agent is thinking. Writes a self-contained game pack that the agentarcade tray app picks up within ten seconds.
---

# Make an agentarcade game

agentarcade shows a small game while the user waits for their agent. This skill
adds a new one, written to disk as a "pack" that the running app discovers on
its own. The app never generates anything itself and has no network access —
you are the generator.

## Before writing anything

Read `REFERENCE.md`, next to this file. It is the contract the game must
satisfy: what the sandbox allows, how scoring is reported, and how the game is
told to stop. A game that ignores it will load and then do nothing.

## Steps

**1. Pick an id.** Lowercase letters, digits and hyphens only, 40 characters
at most, derived from the idea — `dodge falling semicolons` becomes
`semicolon-dodge`. It is both the folder name and the id inside `meta.json`,
and the two must match exactly or the app skips the pack.

**2. Find the games directory.** It is `games/` inside agentarcade's data
directory:

| Platform | Path |
|---|---|
| Windows | `%APPDATA%\com.sirjan.agentarcade\games` |
| macOS | `~/Library/Application Support/com.sirjan.agentarcade/games` |
| Linux | `${XDG_DATA_HOME:-~/.local/share}/com.sirjan.agentarcade/games` |

Create it if it is not there. If `prefs.json` or `stats.json` sits in the
parent directory, you are in the right place.

**3. Write `<games>/<id>/game.html`.** One self-contained file, following
`REFERENCE.md`. Keep it small: one mechanic, one scoring rule, roughly 150
lines. This is a game someone plays for ninety seconds while a build runs, not
a game with a menu.

**4. Write `<games>/<id>/meta.json`:**

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

- `id` must equal the folder name.
- `label` is what the tab bar shows. Two or three words.
- `unit` is rendered next to the score — `pts`, `ms`, `wpm`.
- `lowerIsBetter` is `true` only for times and reaction scores.
- `prompt` is what the user actually asked for, kept verbatim.
- `createdAt` is the current time in ISO 8601.

**5. Tell the user it is ready.** The app rescans every ten seconds, so it
appears in the tab bar without a restart. They can also switch it off, or
delete it, from the Games tab in agentarcade's settings window.

## Checking your work

You cannot run the game inside agentarcade from here, but you can catch most
failures by reading it back:

- Is every script inline, with no `src`, no CDN, no font or image URL?
- Does it call `parent.postMessage({ type: 'agentarcade:score', value: n }, '*')`
  somewhere reachable?
- Does it handle a `message` event of type `agentarcade:freeze`?
- Does it avoid `localStorage`, `sessionStorage`, `fetch` and `XMLHttpRequest`
  entirely? Each one throws or is blocked in the sandbox.
- Does it fill the viewport at a small window size, around 520x360?

If the user reports a blank window, that is almost always one of the first four.
