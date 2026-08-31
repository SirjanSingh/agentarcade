"""Capture README screenshots of each game as the real window looks.

Run against the dev server (`npm run dev`). The dev control bar is stripped
before each shot so the images show the packaged app, not browser mode.
"""

import math
import os
import pathlib
import time

from playwright.sync_api import sync_playwright

URL = os.environ.get("AGENTARCADE_URL", "http://localhost:5183")
OUT = pathlib.Path(__file__).parent.parent / "docs" / "screenshots"
OUT.mkdir(parents=True, exist_ok=True)

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
  return { x: r.left + (sx/n) * (r.width/c.width), y: r.top + (sy/n) * (r.height/c.height) };
}
"""

HIDE_DEVBAR = """
() => {
  document.getElementById('devbar')?.remove();
  document.body.classList.remove('has-devbar');
}
"""


def shot(page, name):
    page.evaluate(HIDE_DEVBAR)
    page.wait_for_timeout(120)
    page.screenshot(path=str(OUT / f"{name}.png"))
    print("wrote", name)


with sync_playwright() as p:
    browser = p.chromium.launch()

    def fresh(dark=True):
        page = browser.new_page(
            viewport={"width": 560, "height": 620},
            color_scheme="dark" if dark else "light",
        )
        page.goto(URL, wait_until="networkidle")
        return page

    # ---- snake: a few moves in
    page = fresh()
    page.select_option("#devbar-game", "snake")
    page.wait_for_selector("canvas.board", timeout=3000)
    for key in ("ArrowDown", "ArrowRight", "ArrowDown", "ArrowLeft"):
        page.keyboard.press(key)
        page.wait_for_timeout(500)
    shot(page, "snake")
    page.close()

    # ---- reaction: caught on green
    page = fresh()
    page.select_option("#devbar-game", "reaction")
    page.wait_for_selector(".reaction-pad", timeout=3000)
    page.click(".reaction-pad")
    deadline = time.time() + 6
    while time.time() < deadline:
        if page.get_attribute(".reaction-pad", "data-phase") == "go":
            break
        page.wait_for_timeout(30)
    shot(page, "reaction")
    page.close()

    # ---- typing: mid-sentence
    page = fresh()
    page.select_option("#devbar-game", "typing")
    page.wait_for_selector(".typing", timeout=3000)
    words = page.evaluate("[...document.querySelectorAll('.typing-stream .word')].map(w => w.textContent)")
    for w in words[:5]:
        page.keyboard.type(w, delay=40)
        page.keyboard.press("Space")
    page.keyboard.type(words[5][:2], delay=60)
    shot(page, "typing")
    page.close()

    # ---- gradient: a shuffled board
    page = fresh()
    page.select_option("#devbar-game", "gradient")
    page.wait_for_selector(".hue-grid", timeout=3000)
    page.wait_for_timeout(200)
    shot(page, "gradient")
    page.close()

    # ---- metronome: mid-pulse with a few taps banked
    page = fresh()
    page.select_option("#devbar-game", "metronome")
    page.wait_for_selector(".metronome-face", timeout=3000)
    for _ in range(6):
        page.keyboard.press("Space")
        page.wait_for_timeout(600)
    page.wait_for_timeout(120)
    shot(page, "metronome")
    page.close()

    # ---- aim: a few targets in
    page = fresh()
    page.select_option("#devbar-game", "aim")
    page.wait_for_selector(".aim-board", timeout=3000)
    for _ in range(5):
        blob = page.evaluate(FIND_ONE_BLOB, [".aim-board", [127, 214, 164], 26, 40])
        if not blob:
            break
        page.mouse.click(blob["x"], blob["y"])
        page.wait_for_timeout(160)
    shot(page, "aim")
    page.close()

    # ---- untangle: partly unpicked
    page = fresh()
    page.select_option("#devbar-game", "untangle")
    page.wait_for_selector(".untangle-board", timeout=3000)
    box = page.locator(".untangle-board").bounding_box()
    node = page.evaluate(FIND_ONE_BLOB, [".untangle-board", [207, 212, 221], 18, 14])
    if node:
        for i in range(3):
            angle = i / 3 * 2 * math.pi
            tx = box["x"] + box["width"] * (0.5 + 0.34 * math.cos(angle))
            ty = box["y"] + box["height"] * (0.5 + 0.34 * math.sin(angle))
            page.mouse.move(node["x"], node["y"])
            page.mouse.down()
            page.mouse.move(tx, ty, steps=6)
            page.mouse.up()
            node = {"x": tx, "y": ty}
            page.wait_for_timeout(120)
    shot(page, "untangle")
    page.close()

    # ---- score card: what Stop looks like
    page = fresh()
    page.select_option("#devbar-game", "reaction")
    page.wait_for_selector(".reaction-pad", timeout=3000)
    page.click(".reaction-pad")
    deadline = time.time() + 6
    while time.time() < deadline:
        if page.get_attribute(".reaction-pad", "data-phase") == "go":
            break
        page.wait_for_timeout(30)
    page.click(".reaction-pad")
    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(200)
    shot(page, "scorecard")
    page.close()

    # ---- stats: needs a history, so play a handful of quick runs first
    page = fresh()
    page.evaluate("localStorage.removeItem('agentarcade.stats')")
    for game, plays in (("reaction", 3), ("metronome", 2), ("aim", 2)):
        for _ in range(plays):
            page.select_option("#devbar-game", game)
            page.wait_for_timeout(300)
            if game == "reaction":
                page.click(".reaction-pad")
                deadline = time.time() + 6
                while time.time() < deadline:
                    if page.get_attribute(".reaction-pad", "data-phase") == "go":
                        break
                    page.wait_for_timeout(30)
                page.click(".reaction-pad")
            elif game == "metronome":
                for _ in range(5):
                    page.keyboard.press("Space")
                    page.wait_for_timeout(600)
            else:
                for _ in range(5):
                    blob = page.evaluate(FIND_ONE_BLOB, [".aim-board", [127, 214, 164], 26, 40])
                    if not blob:
                        break
                    page.mouse.click(blob["x"], blob["y"])
                    page.wait_for_timeout(150)
            page.click("#devbar >> text=Claude done")
            page.wait_for_timeout(1900)
    page.click("#devbar >> text=Stats")
    page.wait_for_selector(".stats", timeout=3000)
    shot(page, "stats")
    page.close()

    # ---- boulder: mid-run, mid-jump, with a token coming at you.
    # Space starts the run and also jumps, so the first press is the start.
    # Shot on the 5th jump: by then the ground is scrolling and an obstacle is
    # on screen, and it still reads as running rather than as the game-over card.
    page = fresh()
    page.select_option("#devbar-game", "boulder")
    page.wait_for_selector("canvas", timeout=3000)
    page.evaluate(HIDE_DEVBAR)
    page.keyboard.press(" ")
    for _ in range(5):
        page.wait_for_timeout(600)
        page.keyboard.press(" ")
    page.wait_for_timeout(140)
    shot(page, "boulder")
    page.close()

    browser.close()
print("\nall screenshots in", OUT)
