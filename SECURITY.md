# Security policy

## Reporting a vulnerability

Please report security problems privately: open the repository's **Security**
tab and click **Report a vulnerability**. Don't open a public issue for them.

You should hear back within a week. If the report holds up, the fix ships in a
release with credit to you, unless you'd rather stay anonymous.

## Supported versions

Only the latest release gets security fixes. The app updates itself from the
tray, so staying current takes one click.

## What is in scope

agentarcade runs on your machine with your user's permissions, so these matter
most:

- **The hook server** on `127.0.0.1:47913`. It should accept requests only from
  local agent hook runners, never from a web page or another machine.
- **The agent config writers.** Connecting an agent edits files like
  `~/.claude/settings.json`. Those writes must never destroy or corrupt a
  user's existing settings.
- **Generated game packs.** A pack runs in a sandboxed iframe and must not be
  able to reach the app's commands, the filesystem, or the network.
- **Short-video windows.** They load remote sites, and must never be given
  access to the app's IPC.
- **The updater**, which must refuse any update not signed with the release key.

The most recent internal review is in
[`docs/security-review-2026-09.md`](docs/security-review-2026-09.md).
