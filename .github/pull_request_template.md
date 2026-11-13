## What and why

## How it was tested

- [ ] `pnpm build`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings`, `cargo test`
- [ ] Tried in dry-run mode (never against the disk your code lives on)
- [ ] Screenshots for UI changes (light and dark)

## Safety

- [ ] No destructive action without confirmation; Herd and Time Machine are only touched when running; no process is killed
