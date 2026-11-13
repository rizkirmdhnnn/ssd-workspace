# Contributing

Thanks for wanting to improve SSD Workspace. This guide covers setting up a development
environment, testing without ejecting your disk, and what a good change looks like. For how
the app works inside, read [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) first.

## Setup

You need macOS 14 or later, [Rust](https://rustup.rs) (stable), [Node.js](https://nodejs.org)
20+ with [pnpm](https://pnpm.io), and the Xcode Command Line Tools.

```sh
pnpm install
pnpm tauri dev      # run the app with hot reload for the Detail window
```

## Test safely: dry-run mode

The eject flow stops Time Machine, stops Herd and ejects a disk. **Never test it for real on
the disk your code lives on.** Use dry-run mode instead:

- **Debug builds always start in dry-run mode.** It can be toggled in the menu under
  **Options → Dry Run Mode** (debug builds only).
- For a release build, set `SSD_WORKSPACE_DRY_RUN=1`:

  ```sh
  SSD_WORKSPACE_DRY_RUN=1 "src-tauri/target/release/bundle/macos/SSD Workspace.app/Contents/MacOS/ssd-workspace"
  ```

In dry-run mode the whole flow runs (status checks, confirmation, `lsof`, Try Again), but
`tmutil stopbackup`, `herd stop` and `diskutil eject` are never executed; the last dialog
lists the commands that *would* have run. A **Dry run** label shows in the menu and the
Detail window.

While dry-run is on, `SSD_WORKSPACE_SIMULATE` (comma-separated) fakes a state, which is handy
for screenshots and UI work:

| Value | Simulates |
| --- | --- |
| `tm` | a Time Machine backup in progress |
| `herd` | the Herd services running |
| `idle` | nothing using the SSD (the app list is empty) |

To exercise mount and unmount handling, use a small disk image instead of your SSD:

```sh
hdiutil create -size 50m -fs APFS -volname Projects /tmp/projects.dmg
hdiutil attach /tmp/projects.dmg      # then detach with: hdiutil detach /Volumes/Projects
```

and point the app at it with a [`config.json`](README.md#configuration).

`SSD_WORKSPACE_TRACE=1` logs every status refresh and the commands it ran.

## Checks

Run these before opening a pull request; CI runs the same.

```sh
pnpm build                                   # TypeScript type check + frontend build
cd src-tauri
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo test                                   # unit tests, all against mocks
```

There are also tests that read the real system. They are ignored by default and never change
anything:

```sh
cargo test real_system_dry_run -- --ignored --nocapture    # the eject flow against the real system
SSD_WORKSPACE_SIMULATE=tm,herd,idle cargo test real_system_dry_run -- --ignored --nocapture
cargo test real_tm_info -- --ignored --nocapture           # Time Machine status and prefs
cargo test real_holders_and_storage -- --ignored --nocapture
```

`real_system_dry_run` uses the real `lsof`, `tmutil` and process table, but every action that
would change the system panics, and the test checks that the disk stays mounted and Herd's
state does not change.

The menu bar icons in `src-tauri/icons/tray/` are rendered from SF Symbols by
`swift scripts/tray-icons.swift`; edit that script rather than the PNGs.

To build the app bundle:

```sh
pnpm tauri build --bundles app
```

## Guidelines

- **Keep the safety rules.** No destructive action without confirmation, never touch Herd or
  Time Machine when they are not running, never kill user processes. See
  [Principles](docs/ARCHITECTURE.md#principles).
- **Keep it idle-cheap.** Anything that runs on every refresh must not spawn a process; prefer
  in-process system calls. If something needs `lsof` or similar, run it on demand.
- **System logic lives in Rust.** The frontend only calls the commands in `src/api.ts`. Run
  commands with `system::cmd` (absolute path, no shell, working directory `/`).
- **Match macOS.** UI text is English and follows Apple's style (title case for menu items,
  "…" for items that open a dialog). The design system is in [DESIGN.md](DESIGN.md).
- **Explain the why.** The code comments say why something is done when it is not obvious
  (a macOS quirk, a safety rule); keep doing that for new code.
- **Small, focused commits** with a clear message, e.g. `fix(eject): …`, `feat(menu): …`,
  `docs: …`.

## Reporting bugs

Open an issue with your macOS version, what you did, what you expected, and what happened.
Output from running the app with `SSD_WORKSPACE_TRACE=1` helps a lot. For security issues, see
[SECURITY.md](SECURITY.md).
