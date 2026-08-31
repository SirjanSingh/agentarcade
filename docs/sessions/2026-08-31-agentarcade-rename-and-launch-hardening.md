# 2026-08-31 — Rename to agentarcade, drop the paid tier, and build the delay

The session that took the app from "works on this machine, private repo" to "a stranger
could install this". Four product decisions, then eight phases of work against them.

## The decisions (made before any code)

Three audit agents swept the codebase first — security, onboarding/customizability, and
release readiness — and the findings drove four choices:

1. **Fully open source, MIT, all 8 games free.** The free/pro split is deleted. A paywall
   on Snake inside an MIT repo is not defensible: anyone forks it out in an afternoon, and
   the audit confirmed the gate was a single boolean (`CLAUDEGAME_PRO=1`, or hand-editing
   `proUnlocked` in stats.json) that the README itself documented. Monetisation, if it ever
   happens, has to be a service that is genuinely hard to copy — cloud sync, team
   leaderboards — not content.
2. **Reputation and revenue, sequenced.** Ship free with a tip jar, measure 60 days, only
   then decide about a paid tier.
3. **Rename now, to `agentarcade`.** Two reasons: Anthropic holds a registered CLAUDE
   trademark (reg. #7645254) and its Software Directory Terms grant no rights to the name,
   and the product is meant to support Cursor/Codex/Gemini, which "claudegame" contradicts.
   Renaming cost an afternoon at zero stars and zero external links; after a public launch
   it would have cost every link, star and install. It also had to happen before the first
   public tag because the Tauri `identifier` determines the app-data directory — changing
   it later would orphan real users' `stats.json`.
4. **1–2 weeks of hardening**, not a weekend dash.

## The thing that actually blocked launch

**A game appeared on every single prompt, including three-second answers.** This was
designed in July and never built; the July notes call it unusable and they were right. It
is the difference between a delightful toy and something a stranger uninstalls in ten
minutes.

**The fix** ([`logic/tracker.rs`](../../src-tauri/src/logic/tracker.rs)): `on_show_event`
no longer shows anything. It records `pending_since` for the session and returns
`Nothing`. `tick` — dropped from 60s to 500ms, since it is now load-bearing for timing —
decides later whether any session has been thinking longer than the configured delay.
A session that stops before its delay elapses never opens a window at all.

Two subtleties worth remembering:

- `pending_since` uses `entry().or_insert()`, not `insert()`. A flicker burst of repeat
  `UserPromptSubmit` events must not keep pushing the start of the bout forward. This is
  also why `SHOW_DEBOUNCE` demoted from primary gate to a cheap boundary guard — the delay
  absorbs the flicker on its own now.
- `start_game` **clears** `pending_since`. Without that, pressing Esc while the agent was
  still thinking would reopen a game on the very next tick, 500ms later.

Verified in the **release build from a clean profile**, not just in tests: a 3-second
answer opens nothing, 13 seconds of thinking opens a game, `Stop` closes it.

## Everything else, by phase

**Phase 0 — CI.** There was no CI except tag-triggered release builds, so nothing ran
`cargo test` on a push. Added [`ci.yml`](../../.github/workflows/ci.yml): cargo test,
clippy `-D warnings`, tsc + vite build, and a game-id drift check. Also added the `license`
field missing from both manifests, regenerated a `package-lock.json` still claiming 0.1.0,
fixed a nanoid advisory, and deleted unused Windows-Store/mobile icon scaffolding.

**Phase 1 — tier removal.** `logic/tiers.rs` → `logic/games.rs` with one flat `GAME_IDS`;
`src/games/free/` and `src/games/pro/` flattened into `src/games/`; `src/tiers.ts`,
`isPro`, `CLAUDEGAME_PRO` and `Stats::pro_unlocked` all gone. Serde ignores the stale
`proUnlocked` key in existing files, so no migration was needed.

**Phase 2 — six security fixes.** In rough order of how much they mattered:

- `tray.rs` wrote the user's `~/.claude/settings.json` with a bare `fs::write` — a crash
  mid-write truncates their real editor config. `stats.rs` already did this correctly, so
  the pattern was extracted into [`atomic_write.rs`](../../src-tauri/src/atomic_write.rs)
  and both now use it.
- `read_settings()` fell back to `json!({})` on *any* parse failure, meaning a malformed
  settings.json would be silently **replaced** with our hooks and nothing else. It now
  returns `Result`, and a toggle on an unparseable file aborts with a native dialog.
- Orphaned hooks: `is_installed` exact-matches the current URL, so changing the port left a
  dead entry costing every prompt a 2s timeout. Added `find_orphaned`/`remove_orphaned`,
  surfaced as an explicit action rather than a silent delete — someone may be running a
  second install on another port deliberately.
- The release CSP allowed `connect-src` to the Vite dev server. Stripped from the base
  config; `tauri.dev.conf.json` + `npm run tauri:dev` carry it for development instead.
- `/event` had no auth. Added a **`Host` header check** — a DNS-rebound page reaches
  loopback but carries its own domain in `Host`, so this closes it. Verified live: 200 for
  a legitimate POST, 403 for a forged Host.
- Startup `.expect()` calls crashed silently in release (no console). Now write a log to
  the temp dir and show a native dialog.

**Deliberately not done: a per-install token on `/event`.** It would live in plaintext in
the same settings.json, inside the same trust boundary, defending nothing the Host check
doesn't — while recreating the orphaned-hook bug on every rotation. The planning agent
independently reached the same conclusion.

**Phase 3 — [`prefs.rs`](../../src-tauri/src/prefs.rs).** There was no preference
persistence at all; pause was in-memory and forgotten on restart. Now holds the delay,
pause, scorecard duration, last hook URL, first-run flag and disabled games.

**Pause is stored as wall-clock epoch millis, not `Instant`.** `Instant` is monotonic and
process-local — it has no meaning across a restart. A 15-minute pause active at shutdown
has to come back as "13 minutes left", not expired and not restarted.

**Phase 4 — setup and diagnostics window.** The app started completely silently and every
failure was invisible: a port conflict meant games simply never appeared, with the reason
going to a stderr release builds don't have. This is a **second window**, built lazily via
`WebviewWindowBuilder` — the game window is borderless, always-on-top, off-taskbar and
hidden, and a settings surface needs the opposite of all four. Status now reports the
endpoint, hook state, path, last hook received, delay and pause.

**Phase 5 — game-id drift.** The TS and Rust lists are hand-synced. Rather than a codegen
step for a list that changes a few times a year,
[`check-game-ids.mjs`](../../scripts/check-game-ids.mjs) fails CI when they disagree.
Verified in both directions.

**Phases 6–7 — rename and docs.** Zero residual `claudegame` references outside this
sessions directory and Cargo.lock. README rewritten, CONTRIBUTING added,
`tests/requirements.txt` + tests README (kept deliberately out of CI — a Python toolchain
plus a browser download plus a dev-server race is real flakiness for little return),
FUNDING.yml for Sponsors + Buy Me a Coffee.

## Two bugs the new tests caught

Both were mine, both found by tests written alongside the code rather than by inspection:

- `Prefs.version` had no `#[serde(default)]`, so any *partial* `prefs.json` failed to parse
  wholesale and silently reverted to defaults. The empty-object test had been passing for
  the wrong reason.
- The welcome window would have reopened on every launch for anyone who set up hooks and
  closed the window without clicking Done. Installing hooks now marks onboarding complete,
  since that is the step setup actually requires.

## Verification

- 92 Rust tests (was 54), clippy clean under `-D warnings`, tsc clean, both bundles build.
- Release binary 7.1 MB, run from a **wiped app-data directory** to get a true first-run.
- Delay, `Host` check, first-run window and Status view all exercised against the running
  release build and screenshotted. Window presence was checked by enumerating visible
  top-level windows via Win32 rather than trusting `MainWindowTitle`, which only reports
  one window per process.
- **Hooks were never installed into the real `~/.claude/settings.json`** — verified
  untouched. That logic has thorough unit tests including a byte-exact round trip, but
  modifying the user's live editor config is theirs to trigger.

## Open items

1. **Repo is still private.** Every README download link 404s for the public until it flips.
2. **`cargo-audit` is not in CI.** 484 transitive crates behind 10 direct dependencies, and
   npm is at 0 advisories but Rust has never been scanned. This is the one genuine gap left
   from the pre-launch security checklist.
3. **The README reads as AI-written** — 14 em dashes, versus 0 in CONTRIBUTING. For an
   agent-adjacent tool aimed at developers who are currently hyper-attuned to this, it is a
   cheap reason to be dismissed. Should go through the `human-writing` skill.
4. **No demo clip.** A 15-second recording of a game appearing during a real wait and
   vanishing on its own is the highest-ROI launch asset and still does not exist.
5. **macOS unverified on hardware.** Its focus-return is a no-op resting on the Accessory
   activation policy; that assumption has never run on a real Mac. README says so plainly.
6. **Unsigned binaries.** SmartScreen and Gatekeeper will cost more installs than every
   security item combined. ~$200–400/yr signing + $99/yr Apple, when it is worth it.
7. **GitHub repo itself still needs renaming** to match, plus description and topics.

---

## Addendum, same day: the three cheap launch items

Picked up items 2, 3 and 4 below after the first push.

**`cargo audit` in CI.** Result worth recording: **zero actual vulnerabilities** across the
484 crates. There are 18 warnings, but 16 are the gtk-rs 0.18 GTK3 bindings Tauri pulls in
for its Linux backend, which are unreachable on Windows and macOS and not fixable here. So
the job runs plain `cargo audit`, **not** `--deny warnings`: a gate that is permanently red
for reasons nobody can act on gets ignored, and then it is not a gate. Real advisories still
fail it. Verified locally against the real lockfile with a prebuilt binary (C: had 1.2 GB
free, so `cargo install` was not an option), then confirmed green on the actual CI run.

Uses `taiki-e/install-action` rather than `rustsec/audit-check`, which wants `issues: write`
and files GitHub issues. That is noise on a one-maintainer repo.

**README rewrite.** 14 em dashes to 0, via the `human-writing` skill. Two structural changes
matter more than the prose: it now opens with what the app *does* rather than what it is, and
the privacy section moved from near the bottom to third, retitled "What it does to your
machine". "No network calls at all, and it backs up your settings.json before touching it" is
the objection a careful reader has to a tray app that edits their editor config. Burying it
was a waste.

**The demo.** `docs/demo.gif` (5.4 MB, 12fps, 800px so GitHub renders it inline) plus
`docs/demo.mp4`. Built with Remotion as stylised UI, not a screen capture, which was the
right call for one reason: it can hold on the beat where a short prompt is answered at 2.9s
and **nothing happens**. A recording of a real session cannot stage that, and a demo that
only showed the game popping would sell the opposite of the product. Palette taken from the
frontend CSS so it matches the app.

It is a promo, not proof the binary works. A real `gdigrab` capture of the release build
during an actual agent wait is still worth doing separately.

The vendored Remotion skill sits at `.claude/skills/remotion-motion-graphics/` and is
deliberately **left untracked** — whether a third-party skill belongs in a public repo is the
maintainer's call.

**Open items now:** 1 (repo still private, still the only blocker), 5, 6 and 7 from the list
above. Items 2, 3 and 4 are done.
