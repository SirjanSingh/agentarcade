# claudegame Pro

This folder holds **Pro-tier games** (Gradient, Metronome, Untangle, Boulder).

They ship in this public repo for v0.1 but are **gated at runtime** — the random
picker only draws from `src/games/free/` unless Pro is unlocked
(`CLAUDEGAME_PRO=1` or a future license key).

A **separate private repo** will publish Pro builds with cloud sync, multi-tool
hooks, and new game packs. This folder is the seam: Pro modules live here, free
modules live in `../free/`.
