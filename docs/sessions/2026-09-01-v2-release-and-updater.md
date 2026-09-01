# 2026-09-01 — v2.0.0: the rename lands, plus in-app updates

The session that turned a hardened private repo into a tagged, signed, shareable release.
Also the session where three separate things failed quietly and only showed up because
something was actually run rather than assumed.

## What shipped

- **Repo renamed** `claudegame` → `agentarcade` on GitHub. Old URL redirects, local remote
  updated, zero residual `claudegame` references in any tracked file including `Cargo.lock`.
- **v2.0.0 tagged.** Major, and it earns it: the Tauri identifier moved, which relocates the
  app-data directory, the free/pro split is gone, and the product is renamed.
- **In-app updates.** Tray → *Check for updates* → download, verify, install, restart.
- **Repo presentation**: description, 12 topics, homepage, three badges, and a 1280×640
  social preview card cut from the tour video's end frame.
- **A shareable test build** for friends: portable exe, installer, an illustrated
  `GUIDE.html`, and a plain-text fallback.
- Old `v1.0.0` / `v1.0.1` releases deleted (they served claudegame-branded installers that
  no longer matched the README). Both tags kept, so history is intact.

## Three failures worth writing down

**1. A passwordless signing key is not a non-interactive signing key.**

The first updater key was generated with no password, on the assumption that "no password"
means "never prompts". It doesn't. The build stopped on
`Info Decrypting updater signing key, expect a prompt for password` and produced **no `.sig`
file at all**, while the bundle step still reported success.

Shipped, that is a release whose `latest.json` no installed client will ever accept, and the
failure is invisible until someone's first update silently does nothing. Regenerated with a
real password and confirmed `agentarcade_2.0.0_x64-setup.exe.sig` actually exists before
tagging. This is the entire reason the public key in `tauri.conf.json` changed mid-session.

**2. The CI cargo cache survived the rename and poisoned the build.**

First push after renaming went red with:

```
failed to read plugin permissions: ... D:\a\claudegame\claudegame\... (os error 3)
```

`tauri-build` writes absolute paths into `target/`, and the runner's checkout path contains
the repo name. The restored cache pointed at a directory that no longer existed. The cache
key now includes `github.event.repository.name`, so a future rename busts the cache instead
of poisoning it. `release.yml` was never affected: it doesn't cache `target/` at all.

**3. `GITHUB_TOKEN` defaulted to read-only.**

All three release jobs failed with `Resource not accessible by integration` on create-release,
despite `permissions: contents: write` being declared at job level. The repo's
*default_workflow_permissions* was `read`. Set to `write` and re-ran.

## Local build notes (Windows)

`cargo.exe` on this machine is an **MSYS symlink to `rustup.exe`**. Git Bash follows it, so
`cargo --version` works and nothing looks wrong; Node cannot, so the Tauri CLI reports
`cargo metadata: program not found`. Prepend the real toolchain directory:

```
D:\rust\rustup\toolchains\stable-x86_64-pc-windows-msvc\bin
```

Also: `CARGO_TARGET_DIR=D:\rust\target` machine-wide, so `src-tauri/target` never exists.
And stop the running app before rebuilding, or the link step dies on a locked exe.

## Verified, not assumed

The release binary was actually run, not just compiled:

- Hook loop end to end: `UserPromptSubmit` → nothing at 4s → game at ~10s → `Stop` → hidden.
- Forged `Host` header still gets a 403.
- Health endpoint responds, version stamps as 2.0.0.
- `GUIDE.html` rendered in a real browser with an automated check for broken images.

## Competitive picture, since it changes the pitch

Searched GitHub across five phrasings plus the web. The direct competitors (play-while-you-
wait) top out around 16–27 stars and most are abandoned; the closest is
`DanWahlin/agent-arcade`, same name, same concept, same Tauri stack.

The **adjacent** category is where the traction is: macOS notch/Dynamic Island panels that
*monitor* agents (`agentbro` 212★, `vibe-island` 141★). Developers apparently feel "I don't
know what my agent is doing" more strongly than "I am bored". Worth remembering when writing
launch copy: the delay and the zero-keypress return are the defensible parts, not the games.

## The release, and the one wart on it

v2.0.0 published after a fourth attempt. The first three failures were the ones above; the
fourth was subtler. **Re-running a failed workflow reuses the token minted when the original
run was created**, so flipping `default_workflow_permissions` to `write` had no effect on a
re-run. A `workflow_dispatch` mints a fresh token and went green immediately.

Assets: Windows NSIS installer **with a valid `.sig`**, Intel and Apple Silicon dmgs, and a
signed `latest.json`.

**`latest.json` covers Windows only.** The bundle targets were `nsis`, `dmg`, `appimage`,
and `dmg` produces no updater artifacts: on macOS `createUpdaterArtifacts` works off the
`app` target, which emits the `.app.tar.gz` and its signature. A Mac user who installs the
2.0.0 dmg therefore gets nothing from *Check for updates* and has to re-download by hand.
The `app` target is added as of `7cd38bf`, but that does not retrofit a published manifest.

**Resolved by re-tagging.** Release and tag deleted, re-tagged on `ff56793`, rebuilt. Cost
nothing because the repo is private with zero installs. The published manifest now reads:

```
darwin-aarch64      sig  agentarcade_aarch64.app.tar.gz
darwin-x86_64       sig  agentarcade_x64.app.tar.gz
windows-x86_64      sig  agentarcade_2.0.0_x64-setup.exe
```

Nine assets, every updater artifact signed. Worth noting the re-tag had to be a real tag
push, not a workflow re-run, for the token reason above.

**Social preview is gated on going public.** GitHub only exposes the upload for a public
repo, or a private one that already has an image, so it cannot be done first. The file is
ready at `docs/social-preview.png` (1280x640, 104 KB against a 1 MB cap).

## Open

1. **The repo is still private.** Deliberate: the choice was to tag and inspect the release
   privately first. Everything else is done, so this is now the only step left.
2. Social preview PNG still needs a manual upload (Settings → General → Social preview).
   There is no API for it.
3. macOS still unverified on real hardware.
4. npm packaging (`optionalDependencies`, per-platform) still unbuilt.
5. The Claude Code community plugin directory is a viable *discovery* listing later, but a
   plugin cannot be the app: it gets no process, tray icon or window.
