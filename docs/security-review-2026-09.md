# Security review, September 2026

Scope: the whole app as of v2.1.0. That covers the hook server, the four agents' config writers, the short-video windows, generated game packs, the updater, and dependencies. It was done before handing a Windows build to friends.

## Fixed in this pass

| Finding | Severity | Fix |
|---|---|---|
| A web page could post to the hook server. The Host check stops DNS rebinding, but `fetch("http://127.0.0.1:47913/event")` from any page carries our own Host. Until now only axum's JSON content-type check stood in the way, and the multi-agent work was about to remove that by accident. | Medium | `server.rs` rejects any request that carries `Origin` or `Sec-Fetch-*` headers. Browsers always send them on cross-site requests; curl and agent hook runners never do. Covered by `a_browser_request_is_refused_even_with_our_host`, and confirmed live: a request with an `Origin` header gets 403. |
| `rustls` 0.23.43 (RUSTSEC-2026-0285, TLS 1.3 handshake). Reached only by the updater's HTTPS fetch. | Medium | Updated to 0.23.45. `cargo audit` now reports no vulnerabilities. |
| Hook commands failed under PowerShell. Codex runs hooks through PowerShell on Windows, where a bare `@-` is a parse error. Reliability rather than security, but it is why the hooks silently did nothing. | n/a | `"@-"` is quoted in every generated command. Verified: `codex exec` reports both hooks `Completed`. |

## Checked and fine

- **Feed windows cannot reach the app.** A probe ran on a live Instagram page inside a feed window and called `__TAURI_INTERNALS__.invoke('set_show_delay', …)`. It was refused, and `prefs.json` did not change. No capability lists a `feed-*` window or has `remote.urls`.
- **Hook commands:** the only value interpolated is a `u16` port. Nothing from a payload, a file or the network reaches a command string.
- **Config writers** for `~/.claude/settings.json`, `~/.codex/hooks.json`, `~/.cursor/hooks.json` and `~/.gemini/config/hooks.json`:
  - They refuse unparseable files (JSONC comments included).
  - They make a one-time `.agentarcade-backup` and write atomically.
  - Uninstall removes only entries whose command posts to `127.0.0.1:<port>/event?src=`.
  - Unit tests cover keeping the user's own hooks, port changes, and exact inverse round trips.
  - The Codex `notify` line in `config.toml` is never touched.
- **Codex hook trust** is left to the user (`/hooks`). The app never writes a trust entry and never passes `--dangerously-bypass-hook-trust`.
- **The log file** (`<app data>/logs/agentarcade.log`) records events and session ids only, never prompts or payload contents. It is capped at 1 MB with one rotated copy.
- **Generated games:** unchanged since the last review. There is a path-traversal allowlist on pack ids, the `agpack:` scheme serves `default-src 'none'`, and the iframe has `sandbox="allow-scripts"`.
- **Other surfaces:**
  - The updater verifies minisign signatures.
  - Feed and browser URLs are compile-time constants.
  - The only `unsafe` is the Win32 foreground-window calls.
- **Automated checks:**
  - `npm audit --omit=dev` finds 0 vulnerabilities.
  - `git log -p` across all branches contains no keys or tokens.
  - `cargo clippy -D warnings` is clean.

## Known and accepted (low)

- **Any local process can still post to the hook server** and pop a window. It is loopback-only, and a local attacker already has more direct options, so the worst case is an unwanted window.
- **The installer is unsigned.** Windows SmartScreen warns on first run. Signing needs a certificate.
- **Eight `cargo audit` warnings** are unmaintained or unsound crates in Tauri's Linux GTK3 backend. They are unreachable on Windows and macOS, and CI already allows them on purpose.
- **Uninstalling the app does not remove its hooks.** A leftover hook fails fast (connection refused), costs nothing noticeable, and **Disconnect** removes it. The friends readme says to disconnect first.
