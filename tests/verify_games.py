"""Exercise the three games in a real browser against the Vite dev server.

Drives the exact event sequence Rust will emit:
    agentarcade.show(id)   ->  game:show
    agentarcade.stop()     ->  game:freeze  (Claude finished)
and checks the score card, score recording and teardown.
"""

import os
import pathlib
import sys
import time
from playwright.sync_api import sync_playwright, expect

URL = os.environ.get("AGENTARCADE_URL", "http://localhost:5183")
SHOT = str(pathlib.Path(__file__).parent / "screenshots")
pathlib.Path(SHOT).mkdir(exist_ok=True)

results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (f"  [{detail}]" if detail else ""))


with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={"width": 560, "height": 620})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.on("console", lambda m: errors.append(f"console.{m.type}: {m.text}") if m.type == "error" else None)

    page.goto(URL, wait_until="networkidle")
    check("page loads", page.title() == "agentarcade", page.title())
    check("browser driver installed", page.evaluate("typeof window.agentarcade === 'object'"))
    check("registry exposes every game", page.evaluate("window.agentarcade.games").__len__() == 8,
          str(page.evaluate("window.agentarcade.games")))

    # ---------------- snake ----------------
    page.evaluate("window.agentarcade.show('snake')")
    page.wait_for_selector("canvas.board", timeout=3000)
    check("snake: label", page.inner_text("#game-label") == "Snake")

    def canvas_hash():
        return page.evaluate("document.querySelector('canvas.board').toDataURL().length && "
                             "document.querySelector('canvas.board').toDataURL().slice(-64)")

    a = canvas_hash()
    page.wait_for_timeout(600)
    b = canvas_hash()
    check("snake: game loop animates", a != b)

    page.keyboard.press("ArrowDown")
    page.wait_for_timeout(400)
    page.keyboard.press("ArrowRight")
    page.wait_for_timeout(400)
    page.screenshot(path=f"{SHOT}/shot_snake.png")

    page.evaluate("window.agentarcade.stop()")
    page.wait_for_timeout(120)
    check("snake: scorecard shown on stop", page.is_visible("#scorecard"))
    frozen_a = canvas_hash()
    page.wait_for_timeout(400)
    check("snake: freeze halts the loop", frozen_a == canvas_hash())
    page.screenshot(path=f"{SHOT}/shot_snake_scorecard.png")

    # auto-hide after the ~1.5s score card
    page.wait_for_timeout(1700)
    check("snake: stage torn down after scorecard",
          page.evaluate("document.getElementById('stage').children.length") == 0)
    check("snake: scorecard hidden after teardown", not page.is_visible("#scorecard"))

    # ---------------- reaction ----------------
    page.evaluate("window.agentarcade.show('reaction')")
    page.wait_for_selector(".reaction-pad", timeout=3000)
    check("reaction: label", page.inner_text("#game-label") == "Reaction")
    check("reaction: starts idle", page.get_attribute(".reaction-pad", "data-phase") == "idle")

    page.click(".reaction-pad")
    check("reaction: arms on click", page.get_attribute(".reaction-pad", "data-phase") == "waiting")

    # too-soon path
    page.click(".reaction-pad")
    check("reaction: penalises early click", page.get_attribute(".reaction-pad", "data-phase") == "early")

    # real attempt: wait for green, then click
    page.click(".reaction-pad")
    deadline = time.time() + 6
    while time.time() < deadline:
        if page.get_attribute(".reaction-pad", "data-phase") == "go":
            break
        page.wait_for_timeout(30)
    check("reaction: reaches green", page.get_attribute(".reaction-pad", "data-phase") == "go")
    page.click(".reaction-pad")
    headline = page.inner_text(".reaction-headline")
    check("reaction: reports a time", headline.endswith("ms"), headline)
    page.screenshot(path=f"{SHOT}/shot_reaction.png")

    page.evaluate("window.agentarcade.stop()")
    page.wait_for_timeout(150)
    score_txt = page.inner_text("#final-score")
    check("reaction: scorecard carries the ms score", score_txt.endswith("ms"), score_txt)
    page.wait_for_timeout(1700)

    stats = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats'))")
    check("reaction: result persisted", stats["games"].get("reaction", {}).get("plays") == 1, str(stats.get("games")))
    check("reaction: best recorded", isinstance(stats["games"]["reaction"]["best"], (int, float)))

    # ---------------- typing ----------------
    page.evaluate("window.agentarcade.show('typing')")
    page.wait_for_selector(".typing", timeout=3000)
    check("typing: label", page.inner_text("#game-label") == "Typing")
    check("typing: prompt before first key", "start typing" in page.inner_text(".typing-meta"))

    words = page.evaluate("[...document.querySelectorAll('.typing-stream .word')].map(w => w.textContent)")
    typed_words = words[:8]
    for w in typed_words:
        page.keyboard.type(w, delay=45)
        page.keyboard.press("Space")
    meta = page.inner_text(".typing-meta")
    check("typing: live wpm + accuracy", "wpm" in meta and "acc" in meta, meta)
    check("typing: advances through words",
          page.evaluate("document.querySelector('.typing-stream .word.active').textContent") not in typed_words[:1])
    page.screenshot(path=f"{SHOT}/shot_typing.png")

    page.evaluate("window.agentarcade.stop()")
    page.wait_for_timeout(150)
    score_txt = page.inner_text("#final-score")
    sub_txt = page.inner_text("#final-sub")
    check("typing: scorecard shows wpm", score_txt.endswith("wpm"), score_txt)
    check("typing: wpm is a real number, not 0", int(score_txt.split()[0]) > 0, score_txt)
    check("typing: first run flagged as a best", "first run" in sub_txt, sub_txt)
    page.screenshot(path=f"{SHOT}/shot_typing_scorecard.png")
    page.wait_for_timeout(1700)

    # A run too short to measure must record nothing rather than a bogus 0 wpm
    # that would install itself as a permanent personal best.
    before = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats')).games.typing.plays")
    page.evaluate("window.agentarcade.show('typing')")
    page.wait_for_selector(".typing", timeout=3000)
    page.keyboard.type("time", delay=5)
    page.evaluate("window.agentarcade.stop()")
    page.wait_for_timeout(150)
    check("typing: sub-second run scores nothing",
          page.inner_text("#final-score") == "—", page.inner_text("#final-score"))
    page.wait_for_timeout(1700)
    after = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats')).games.typing.plays")
    check("typing: sub-second run not persisted", before == after, f"{before} -> {after}")

    # ---------------- Esc dismissal ----------------
    page.evaluate("window.agentarcade.show('reaction')")
    page.wait_for_selector(".reaction-pad", timeout=3000)
    page.click(".reaction-pad")
    deadline = time.time() + 6
    while time.time() < deadline:
        if page.get_attribute(".reaction-pad", "data-phase") == "go":
            break
        page.wait_for_timeout(30)
    page.click(".reaction-pad")
    page.keyboard.press("Escape")
    page.wait_for_timeout(250)
    check("esc: tears the stage down", page.evaluate("document.getElementById('stage').children.length") == 0)
    stats = page.evaluate("JSON.parse(localStorage.getItem('agentarcade.stats'))")
    check("esc: run still recorded", stats["games"]["reaction"]["plays"] == 2, str(stats["games"]["reaction"]))

    # ---------------- personal best surfaces on the next run ----------------
    page.evaluate("window.agentarcade.show('typing')")
    page.wait_for_selector(".typing", timeout=3000)
    best_line = page.inner_text("#best")
    check("best score surfaces in status bar", best_line.startswith("best ") and "wpm" in best_line, best_line)

    # ---------------- dev bar ----------------
    check("devbar: rendered in browser mode", page.is_visible("#devbar"))
    opts = page.evaluate("[...document.querySelectorAll('#devbar-game option')].map(o => o.value)")
    check("devbar: dropdown lists every game",
          opts == ["", "snake", "reaction", "typing", "gradient", "metronome", "aim", "untangle", "boulder"], str(opts))
    labels = page.evaluate("[...document.querySelectorAll('#devbar-game option')].map(o => o.textContent)")
    check("devbar: uses the games' own labels",
          labels[1:] == ["Snake", "Reaction", "Typing", "Gradient", "Metronome", "Aim", "Untangle", "Boulder"], str(labels))

    page.select_option("#devbar-game", "snake")
    page.wait_for_selector("canvas.board", timeout=3000)
    check("devbar: selecting a game starts it", page.inner_text("#game-label") == "Snake")
    page.screenshot(path=f"{SHOT}/shot_devbar.png")

    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(150)
    check("devbar: 'Claude done' runs the score card", page.is_visible("#scorecard"))
    check("devbar: score card leaves the bar reachable", page.is_visible("#devbar"))
    page.wait_for_timeout(1700)

    page.select_option("#devbar-game", "")
    page.wait_for_timeout(300)
    check("devbar: random option picks something",
          page.inner_text("#game-label") in
          ("Snake", "Reaction", "Typing", "Gradient", "Metronome", "Aim", "Untangle", "Boulder"),
          page.inner_text("#game-label"))

    page.evaluate("window.agentarcade.hide()")
    page.wait_for_timeout(200)
    first = page.evaluate("""() => {
      window.agentarcade.show();
      return document.getElementById('game-label').textContent;
    }""")
    second = page.evaluate("""() => {
      window.agentarcade.show();
      return document.getElementById('game-label').textContent;
    }""")
    check("random show: no back-to-back repeat", first != second, f"{first} -> {second}")
    page.click("#devbar >> text=Hide")
    page.wait_for_timeout(200)
    check("devbar: Hide dismisses without a score card", not page.is_visible("#scorecard"))

    page.click("#devbar >> text=Reset stats")
    page.wait_for_timeout(200)
    check("devbar: reset clears bests",
          page.evaluate("localStorage.getItem('agentarcade.stats')") is None)
    check("devbar: bests readout updates", "no bests yet" in page.inner_text(".devbar-bests"),
          page.inner_text(".devbar-bests"))

    check("no page errors", not errors, "; ".join(errors[:3]))
    browser.close()

passed = sum(1 for _, ok, _ in results if ok)
print(f"\n{passed}/{len(results)} checks passed")
sys.exit(0 if passed == len(results) else 1)
