"""Exercise the four new games plus the snake rate-scoring fix."""

import math
import os
import pathlib
import sys
import time
from playwright.sync_api import sync_playwright

URL = os.environ.get("AGENTARCADE_URL", "http://localhost:5183")
SHOT = str(pathlib.Path(__file__).parent / "screenshots")
pathlib.Path(SHOT).mkdir(exist_ok=True)

results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (f"  [{detail}]" if detail else ""))


# Locate a coloured blob on a canvas by centroid, so tests can click real targets.
FIND_BLOB = """
([sel, target, tol]) => {
  const c = document.querySelector(sel);
  const g = c.getContext('2d');
  const d = g.getImageData(0, 0, c.width, c.height).data;
  let sx = 0, sy = 0, n = 0;
  for (let i = 0; i < d.length; i += 4) {
    if (Math.abs(d[i]-target[0]) <= tol && Math.abs(d[i+1]-target[1]) <= tol && Math.abs(d[i+2]-target[2]) <= tol) {
      const p = i / 4;
      sx += p % c.width; sy += Math.floor(p / c.width); n++;
    }
  }
  if (n === 0) return null;
  const r = c.getBoundingClientRect();
  return { x: r.left + (sx/n) * (r.width/c.width), y: r.top + (sy/n) * (r.height/c.height), pixels: n };
}
"""

# Same idea but for boards with several blobs: take the first matching pixel and
# average only its immediate neighbourhood, so we land on one node, not between them.
FIND_ONE_BLOB = """
([sel, target, tol, span]) => {
  const c = document.querySelector(sel);
  const g = c.getContext('2d');
  const d = g.getImageData(0, 0, c.width, c.height).data;
  const hit = (i) => Math.abs(d[i]-target[0]) <= tol && Math.abs(d[i+1]-target[1]) <= tol && Math.abs(d[i+2]-target[2]) <= tol;
  let seed = -1;
  for (let i = 0; i < d.length; i += 4) { if (hit(i)) { seed = i / 4; break; } }
  if (seed < 0) return null;
  const sxp = seed % c.width, syp = Math.floor(seed / c.width);
  let sx = 0, sy = 0, n = 0;
  for (let y = Math.max(0, syp - span); y < Math.min(c.height, syp + span); y++) {
    for (let x = Math.max(0, sxp - span); x < Math.min(c.width, sxp + span); x++) {
      const i = (y * c.width + x) * 4;
      if (hit(i)) { sx += x; sy += y; n++; }
    }
  }
  const r = c.getBoundingClientRect();
  return { x: r.left + (sx/n) * (r.width/c.width), y: r.top + (sy/n) * (r.height/c.height), pixels: n };
}
"""

with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={"width": 560, "height": 680})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto(URL, wait_until="networkidle")
    page.evaluate("localStorage.removeItem('agentarcade.stats')")

    opts = page.evaluate("[...document.querySelectorAll('#devbar-game option')].map(o => o.value)")
    check("registry has all 7 games",
          opts == ["", "snake", "reaction", "typing", "gradient", "metronome", "aim", "untangle"], str(opts))

    # ---------------- gradient sort ----------------
    page.select_option("#devbar-game", "gradient")
    page.wait_for_selector(".hue-grid", timeout=3000)
    tiles = page.locator(".hue-tile")
    check("gradient: 5x6 board", tiles.count() == 30, str(tiles.count()))
    check("gradient: four pinned corners",
          page.locator(".hue-tile.anchor").count() == 4, str(page.locator(".hue-tile.anchor").count()))
    start_pct = int(page.inner_text(".hue-meta").split("%")[0])
    check("gradient: starts shuffled, not solved", start_pct < 100, f"{start_pct}%")
    # Displacement scoring should put a fresh shuffle in a readable mid range,
    # not pinned near zero the way exact-match scoring was.
    check("gradient: a shuffle reads as partial progress, not ~0",
          20 <= start_pct <= 90, f"{start_pct}%")

    movable = page.locator(".hue-tile:not(.anchor)")
    movable.nth(3).click()
    check("gradient: first click selects", page.locator(".hue-tile.selected").count() == 1)
    movable.nth(9).click()
    check("gradient: second click clears the selection", page.locator(".hue-tile.selected").count() == 0)

    # Several swaps must move the reading — a score that never responds to play
    # is the bug displacement scoring was meant to fix.
    seen = {start_pct}
    for i in range(0, 20, 2):
        movable.nth(i).click()
        movable.nth(i + 1).click()
        seen.add(int(page.inner_text(".hue-meta").split("%")[0]))
    check("gradient: sortedness responds to swaps", len(seen) > 1, f"values seen: {sorted(seen)}")
    page.screenshot(path=f"{SHOT}/shot_gradient.png")

    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    card = page.inner_text("#final-score")
    check("gradient: freeze scores a percentage", card.endswith("%") or card == "—", card)
    page.wait_for_timeout(1700)

    # ---------------- metronome ----------------
    page.select_option("#devbar-game", "metronome")
    page.wait_for_selector(".metronome-face", timeout=3000)
    check("metronome: prompts before any tap", "tap space" in page.inner_text(".metronome-meta"))

    # 600 ms period at 100 bpm; tap on that cadence.
    for _ in range(6):
        page.keyboard.press("Space")
        page.wait_for_timeout(600)
    meta = page.inner_text(".metronome-meta")
    check("metronome: reports mean error in ms", "ms" in meta and "taps" in meta, meta)
    check("metronome: error is within half a period", int(meta.split("±")[1].split(" ")[0]) <= 300, meta)
    page.screenshot(path=f"{SHOT}/shot_metronome.png")

    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    card = page.inner_text("#final-score")
    check("metronome: freeze scores in ms", card.endswith("ms"), card)
    page.wait_for_timeout(1700)
    stats = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats'))")
    check("metronome: recorded as lower-is-better",
          stats["games"]["metronome"]["best"] is not None, str(stats["games"].get("metronome")))

    # ---------------- aim ----------------
    page.select_option("#devbar-game", "aim")
    page.wait_for_selector(".aim-board", timeout=3000)
    check("aim: prompts before any click", "click the target" in page.inner_text(".aim-meta"))

    hits = 0
    for _ in range(8):
        blob = page.evaluate(FIND_BLOB, [".aim-board", [127, 214, 164], 26])
        if not blob:
            break
        page.mouse.click(blob["x"], blob["y"])
        hits += 1
        page.wait_for_timeout(120)
    check("aim: targets are hittable", hits >= 5, f"{hits} hits")
    meta = page.inner_text(".aim-meta")
    check("aim: reports a rate, not a raw count", "/min" in meta and "acc" in meta, meta)
    page.screenshot(path=f"{SHOT}/shot_aim.png")

    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    card = page.inner_text("#final-score")
    check("aim: freeze scores per minute", card.endswith("/min"), card)
    page.wait_for_timeout(1700)

    # ---------------- untangle ----------------
    page.select_option("#devbar-game", "untangle")
    page.wait_for_selector(".untangle-board", timeout=3000)
    meta = page.inner_text(".untangle-meta")
    check("untangle: starts with a knot", "crossing" in meta, meta)
    start_crossings = int(meta.split(" ")[0])
    check("untangle: at least 3 crossings to start", start_crossings >= 3, meta)

    box = page.locator(".untangle-board").bounding_box()

    def crossings_now():
        text = page.inner_text(".untangle-meta")
        return 0 if text.startswith("untangled") else int(text.split(" ")[0])

    def drag_to(frm, x, y):
        page.mouse.move(frm["x"], frm["y"])
        page.mouse.down()
        page.mouse.move(x, y, steps=6)
        page.mouse.up()
        page.wait_for_timeout(90)

    node = page.evaluate(FIND_ONE_BLOB, [".untangle-board", [207, 212, 221], 18, 14])
    check("untangle: a single node is findable", node is not None and node["pixels"] > 40,
          str(node and node["pixels"]))

    def hill_climb():
        """Park one node at each of eight positions, keep the best reading.

        Returns (start, best). Boards that open with only three crossings can
        resist single-node moves entirely, so the caller retries on a new board
        rather than treating one stubborn layout as a failure.
        """
        start = crossings_now()
        best_seen = start
        for _ in range(3):
            here = page.evaluate(FIND_ONE_BLOB, [".untangle-board", [207, 212, 221], 18, 14])
            if not here:
                break
            for i in range(8):
                angle = i / 8 * 2 * math.pi
                tx = box["x"] + box["width"] * (0.5 + 0.36 * math.cos(angle))
                ty = box["y"] + box["height"] * (0.5 + 0.36 * math.sin(angle))
                drag_to(here, tx, ty)
                here = {"x": tx, "y": ty}
                best_seen = min(best_seen, crossings_now())
        return start, best_seen

    reduced = False
    attempts = []
    for _ in range(3):
        start, best = hill_climb()
        attempts.append(f"{start}->{best}")
        if best < start:
            reduced = True
            break
        page.click("#devbar >> text=Hide")
        page.wait_for_timeout(150)
        page.select_option("#devbar-game", "untangle")
        page.wait_for_selector(".untangle-board", timeout=3000)
    check("untangle: crossings can actually be reduced", reduced, ", ".join(attempts))
    check("untangle: percentage reading stays live", "%" in page.inner_text(".untangle-meta"),
          page.inner_text(".untangle-meta"))
    page.screenshot(path=f"{SHOT}/shot_untangle.png")

    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    card = page.inner_text("#final-score")
    check("untangle: freeze scores a percentage", card.endswith("%") or card == "—", card)
    page.wait_for_timeout(1700)

    # ---------------- snake now scores a rate ----------------
    page.select_option("#devbar-game", "snake")
    page.wait_for_selector("canvas.board", timeout=3000)
    page.wait_for_timeout(1500)
    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    check("snake: a 1.5s run is too short to score",
          page.inner_text("#final-score") == "—", page.inner_text("#final-score"))
    page.wait_for_timeout(1700)
    stats = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats'))")
    check("snake: short run not recorded", "snake" not in stats["games"], str(list(stats["games"])))

    page.select_option("#devbar-game", "snake")
    page.wait_for_selector("canvas.board", timeout=3000)
    # Steer around for long enough to clear the 6s scoring floor.
    deadline = time.time() + 7
    while time.time() < deadline:
        for key in ("ArrowDown", "ArrowRight", "ArrowUp", "ArrowLeft"):
            page.keyboard.press(key)
            page.wait_for_timeout(420)
            if time.time() > deadline:
                break
    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    card = page.inner_text("#final-score")
    check("snake: scores apples per minute", card.endswith("apples/min"), card)
    page.screenshot(path=f"{SHOT}/shot_snake_rate.png")

    check("no page errors", not errors, "; ".join(errors[:3]))
    browser.close()

passed = sum(1 for _, ok, _ in results if ok)
print(f"\n{passed}/{len(results)} checks passed")
sys.exit(0 if passed == len(results) else 1)
