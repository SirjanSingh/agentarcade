"""Exercise the stats screen and the v2 schema."""

import os
import pathlib
import sys
from playwright.sync_api import sync_playwright

URL = os.environ.get("CLAUDEGAME_URL", "http://localhost:5183")
SHOT = str(pathlib.Path(__file__).parent / "screenshots")
pathlib.Path(SHOT).mkdir(exist_ok=True)

results = []


def check(name, cond, detail=""):
    results.append((name, bool(cond), detail))
    print(("PASS " if cond else "FAIL ") + name + (f"  [{detail}]" if detail else ""))


with sync_playwright() as p:
    browser = p.chromium.launch()
    page = browser.new_page(viewport={"width": 560, "height": 680})
    errors = []
    page.on("pageerror", lambda e: errors.append(str(e)))
    page.goto(URL, wait_until="networkidle")
    page.evaluate("localStorage.removeItem('claudegame.stats')")

    # Empty state first.
    page.click("#devbar >> text=Stats")
    page.wait_for_selector(".stats", timeout=3000)
    check("stats: empty state explains itself", "no runs yet" in page.inner_text(".stats-empty"),
          page.inner_text(".stats-empty"))
    check("stats: header shows the view name", page.inner_text("#game-label") == "Stats")

    # A v1 file must upgrade in place, keeping its bests.
    page.evaluate("""
      localStorage.setItem('claudegame.stats', JSON.stringify({
        version: 1,
        games: { snake: { plays: 7, best: 19, total: 54, lastPlayed: '2026-07-01T00:00:00.000Z' } },
        totals: { sessions: 7, secondsPlayed: 300 }
      }));
    """)
    page.click("#devbar >> text=Stats")
    page.wait_for_selector(".stats-row", timeout=3000)
    upgraded = page.evaluate("JSON.parse(localStorage.getItem('claudegame.stats'))")
    check("stats: v1 bests survive being read", "19" in page.inner_text(".stats-row"),
          page.inner_text(".stats-row").replace("\n", " "))
    check("stats: v1 totals survive", "7 runs" in page.inner_text(".stats-totals"),
          page.inner_text(".stats-totals"))

    # Play a few runs so the history and sparkline have something to show.
    for _ in range(3):
        page.select_option("#devbar-game", "metronome")
        page.wait_for_selector(".metronome-face", timeout=3000)
        for _ in range(5):
            page.keyboard.press("Space")
            page.wait_for_timeout(600)
        page.click("#devbar >> text=Claude done")
        page.wait_for_timeout(1900)

    stats = page.evaluate("JSON.parse(localStorage.getItem('claudegame.stats'))")
    check("schema: version is 2", stats["version"] == 2, str(stats["version"]))
    check("schema: device id minted", isinstance(stats.get("deviceId"), str) and len(stats["deviceId"]) > 8,
          str(stats.get("deviceId")))
    check("schema: runs recorded with ids and timestamps",
          len(stats["recent"]) == 3 and all(r.get("id") and r.get("at") for r in stats["recent"]),
          str(len(stats["recent"])))
    check("schema: newest run first",
          stats["recent"][0]["at"] >= stats["recent"][-1]["at"], "")
    ids = {r["id"] for r in stats["recent"]}
    check("schema: run ids are unique", len(ids) == len(stats["recent"]), str(len(ids)))

    device_before = stats["deviceId"]
    page.reload(wait_until="networkidle")
    after = page.evaluate("JSON.parse(localStorage.getItem('claudegame.stats'))")
    check("schema: device id is stable across reloads", after["deviceId"] == device_before)

    page.click("#devbar >> text=Stats")
    page.wait_for_selector(".stats-row", timeout=3000)
    rows = page.locator(".stats-row").count()
    check("stats: one row per played game", rows == 2, f"{rows} rows")
    check("stats: sparkline drawn once there are 2+ runs",
          page.locator(".stats-row[data-game=metronome] .spark polyline").count() == 1)
    check("stats: recent list rendered", page.locator(".stats-recent li").count() == 3,
          str(page.locator(".stats-recent li").count()))
    check("stats: recent entries are timestamped",
          "ago" in page.inner_text(".stats-recent") or "just now" in page.inner_text(".stats-recent"),
          page.inner_text(".stats-recent").split("\n")[0])
    check("stats: says data stays local", "nothing leaves this machine" in page.inner_text(".stats-footer"))
    page.screenshot(path=f"{SHOT}/shot_stats.png")

    # Claude finishing while the stats screen is up must just close it.
    page.click("#devbar >> text=Claude done")
    page.wait_for_timeout(250)
    check("stats: Claude finishing dismisses without a score card",
          not page.is_visible("#scorecard") and page.evaluate("document.getElementById('stage').children.length") == 0)

    check("no page errors", not errors, "; ".join(errors[:3]))
    browser.close()

passed = sum(1 for _, ok, _ in results if ok)
print(f"\n{passed}/{len(results)} checks passed")
sys.exit(0 if passed == len(results) else 1)
