# The agentarcade game pack contract

A generated game is one HTML file. agentarcade serves it into an iframe that is
sandboxed with `allow-scripts` and **without** `allow-same-origin`, and serves
it under its own Content-Security-Policy:

```
default-src 'none'; script-src 'unsafe-inline'; style-src 'unsafe-inline'; img-src data:
```

That policy is enforced by the app, not by convention. Everything below follows
from it.

## The rules

**One file, nothing external.** No `<script src>`, no `<link rel=stylesheet>`,
no web fonts, no images by URL. Inline every script and style. If you need
artwork, draw it on a canvas or embed it as a `data:` URI.

**No network.** `fetch`, `XMLHttpRequest`, `WebSocket` and `EventSource` are all
blocked by `default-src 'none'`. agentarcade's whole promise is that nothing
leaves the user's machine; do not write code that tries.

**No storage.** The frame has an opaque origin, so `localStorage`,
`sessionStorage` and `indexedDB` throw on access — not on write, on *access*.
Touching them will break the game. Personal bests are handled by the app; you do
not need to persist anything.

**Fill the viewport.** The window is small and resizable. Assume something like
520x360 and lay out relative to `innerWidth`/`innerHeight`, not fixed pixels.
Handle `resize` if the game has a play field.

**One mechanic.** No start screen, no menu, no level select, no pause. The game
begins the moment it loads, because the user opened it by waiting on a build,
not by choosing to play. Roughly 150 lines is the target.

## Reporting a score

Post the score to the parent window whenever it changes. The app keeps the
latest value and records it when the run ends.

```js
function report(score) {
  parent.postMessage({ type: 'agentarcade:score', value: score }, '*');
}
```

`value` must be a finite number. Send it as often as you like — every point, or
once at the end for a game that only produces a result when it finishes.

Score in something that measures the player, not how long their agent took.
A game that counts total hits just measures the length of the wait; hits per
minute measures aim. If the natural score is a time, set `lowerIsBetter` to
`true` in `meta.json`.

## Stopping

When the user's agent finishes, the app tells the game to stop so the final
frame is still on screen behind the score card:

```js
addEventListener('message', (e) => {
  if (e.data && e.data.type === 'agentarcade:freeze') {
    running = false;            // stop your loop
    cancelAnimationFrame(raf);  // and your timers
  }
});
```

Leave the last frame drawn. Do not clear the canvas, show a game-over screen, or
navigate — the app draws its own score card over the top.

## A minimal complete example

```html
<meta charset="utf-8">
<style>
  html, body { margin: 0; height: 100%; background: #11131a; overflow: hidden; }
  canvas { display: block; width: 100%; height: 100%; }
</style>
<canvas id="c"></canvas>
<script>
  const canvas = document.getElementById('c');
  const ctx = canvas.getContext('2d');
  let running = true, raf = 0, score = 0;

  function resize() {
    canvas.width = innerWidth;
    canvas.height = innerHeight;
  }
  addEventListener('resize', resize);
  resize();

  addEventListener('message', (e) => {
    if (e.data && e.data.type === 'agentarcade:freeze') {
      running = false;
      cancelAnimationFrame(raf);
    }
  });

  function frame() {
    if (!running) return;
    // ... update and draw ...
    parent.postMessage({ type: 'agentarcade:score', value: score }, '*');
    raf = requestAnimationFrame(frame);
  }
  raf = requestAnimationFrame(frame);
</script>
```

## Style

The app's own games are dark, flat and quiet: a near-black background, one
accent colour, no textures and no sound. Match that unless the user asks for
something else — the game opens over their editor while they are working, and
anything loud is worse than no game at all.
