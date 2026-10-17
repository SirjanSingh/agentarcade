## What this changes

<!-- One or two sentences. Link the issue it closes, e.g. "Closes #12". -->

## How I checked it

<!-- For a game or any UI change: a screenshot or short gif of it running. -->

- [ ] `npm run build` passes
- [ ] `npm run test:rust` passes
- [ ] `npm run check:game-ids` passes (if I added or renamed a game)
- [ ] `cargo clippy --manifest-path src-tauri/Cargo.toml --all-targets -- -D warnings` is clean
- [ ] Behaviour changes in `tracker.rs` come with a test that fails without them
