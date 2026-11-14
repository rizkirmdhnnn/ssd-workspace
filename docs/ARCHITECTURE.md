# Architecture

How SSD Workspace works inside. For what it does and how to use it, see the
[README](../README.md); for setting up a development environment, see
[CONTRIBUTING](../CONTRIBUTING.md).

## Overview

SSD Workspace is a [Tauri 2](https://tauri.app) app with almost all of its logic in Rust.
The only web UI is the Detail window (Preact + TypeScript, about 22 kB of JavaScript); the
menu bar icon and its menu are native.

```
                    ┌─────────────────────────── Rust (src-tauri/src) ───────────────────────────┐
 kqueue mount   ──▶ │  monitor thread (lib.rs) ──▶ Snapshot ──▶ tray.rs + menu_style.rs  (menu)   │
 notifications      │        │  tmutil status, defaults export (timemachine.rs)                   │
                    │        │  statfs, getattrlist, libproc (system.rs, holders.rs)              │
                    │        └──▶ "status-changed" event ─────────────────────┐                  │
                    │                                                         ▼                  │
                    │  commands: get_status, list_holders, eject_ssd, …  ◀── Detail window (src/) │
                    │  eject flow (eject.rs) ──▶ dialogs, tmutil stopbackup, herd stop, diskutil │
                    └────────────────────────────────────────────────────────────────────────────┘
```

## Modules

| File | Responsibility |
| --- | --- |
| [`lib.rs`](../src-tauri/src/lib.rs) | App state, the monitor thread, dialogs, the Detail window lifecycle, and the commands the frontend calls |
| [`config.rs`](../src-tauri/src/config.rs) | Which volumes to manage (`config.json`, defaults `Workspace` / `Backup`) |
| [`system.rs`](../src-tauri/src/system.rs) | Low-level system access: mount checks, `statfs`, kqueue mount notifications, the process table, `lsof`, Herd, running commands safely |
| [`holders.rs`](../src-tauri/src/holders.rs) | Which apps hold the SSD (`lsof` grouped by `.app` ancestor, app icons) and the APFS container breakdown |
| [`timemachine.rs`](../src-tauri/src/timemachine.rs) | Time Machine status, progress, latest backup, and the Time Machine menu actions |
| [`eject.rs`](../src-tauri/src/eject.rs) | The eject flow, written against an `Env` trait so it can be tested with a mock |
| [`tray.rs`](../src-tauri/src/tray.rs) | The menu bar icon and menu |
| [`menu_style.rs`](../src-tauri/src/menu_style.rs) | Native menu styling Tauri cannot express (header with subtitle, section header) and the eject-readiness check |
| [`menu_icons.rs`](../src-tauri/src/menu_icons.rs) | SF Symbols on menu items, including the macOS 27 visibility fix |
| [`src/`](../src) | The Detail window: `App.tsx` (UI), `api.ts` (typed wrappers for the Rust commands), `icons.tsx`, `styles.css` |

## Principles

These rules come before convenience; please keep them when changing the code.

1. **Never eject without confirmation.** Every eject goes through a confirmation dialog.
2. **Never touch what is not running.** Herd is only stopped if its services are running
   (calling the `herd` CLI would otherwise start it). A Time Machine backup is only stopped if
   one is in progress.
3. **Never kill user processes.** If an app still holds the SSD, the user is told which one
   and can try again. "Show" only brings that app to the front.
4. **Cost nothing while idle.** The app runs all day on a laptop, often on battery.
5. **No shell.** Every command is run directly with `std::process::Command`, by absolute
   path, with arguments as separate strings and the working directory set to `/`.

## The monitor

One background thread ([`start_monitor`](../src-tauri/src/lib.rs)) owns all status reading.
It builds a `Snapshot` (mount state, free space, Time Machine, eject in progress, settings),
and when the snapshot changes it updates the menu and, if the Detail window is visible,
sends it a `status-changed` event.

It wakes up when:

- a volume is mounted or unmounted (kqueue `EVFILT_FS`, see `system::watch_mounts`), after a
  250 ms settle delay;
- something asks for a refresh (`request_refresh`: the menu, the Detail window, an eject);
- the poll interval elapses: 30 s when idle, 3 s while a backup or an eject is running.

The only thing that has to be polled is Time Machine (`tmutil status -X`, one child process
per interval). Its preferences (`defaults export /Library/Preferences/com.apple.TimeMachine`,
for the latest backup date) only change when a backup finishes, so they are cached and
re-read then, and at most every 10 minutes otherwise. Everything else is read in-process:
mount checks with `stat`, free space with `statfs` and `getattrlist`, Herd detection by
walking the process table with libproc.

`SSD_WORKSPACE_TRACE=1` logs every refresh, why it happened, and which commands it ran.

## Who is holding the SSD

[`holders.rs`](../src-tauri/src/holders.rs) runs one `lsof -Fpcfn` over the configured
volumes, then walks each process's parent chain (`proc_pidinfo`) up to the nearest process
whose executable lives inside an `.app` bundle. Processes are grouped by that bundle, so a
`bash` inside Terminal counts as Terminal and a `node` started by an editor counts as that
editor. Processes with no app ancestor are listed under their own name. System processes
that always touch volumes (`mds`, `fseventsd`, `backupd`, …) and this app itself are ignored.

App icons come from `NSWorkspace iconForFile:`, rendered once per app to a 64 px PNG and
sent to the web view as a data URL.

`lsof` is too costly for the regular refresh, so it only runs when the menu opens (at most
every 5 seconds, in the background), when the Detail window opens or gains focus, on Refresh,
and after the SSD is plugged in or ejected. The last result is cached, so the Detail window
can show a verdict immediately.

## The eject flow

[`eject.rs`](../src-tauri/src/eject.rs) talks to the system only through the `Env` trait, so
the whole flow is unit-tested against a mock, and an ignored test runs it against the real
system with every state-changing action replaced by a `panic!`.

Eject is disabled (menu item, Detail button and `start_eject` itself) while Time Machine is
backing up to the backup volume on the SSD (`Snapshot::backing_up_to_ssd`): stopping it would
cut the backup short. The steps below still stop a backup that targets another disk or that
started after the eject began.

1. The workspace volume is not mounted → an info dialog, done.
2. Confirmation. The dialog names the volumes that will be unmounted, says whether Herd will
   be stopped, and warns when a Time Machine backup is running (the button then reads
   **Stop Backup & Eject**).
3. On a background thread:
   - Time Machine running → `tmutil stopbackup`, then wait (up to ~30 s) until it stops.
   - Herd services running → `herd stop`. If the Herd app was closed before and the CLI
     launched it, it is quit again (a normal quit, never a kill).
   - `lsof` over the volumes. If anything is still holding them, a dialog lists it with
     **Try Again** / **Cancel**.
   - `diskutil eject /Volumes/<workspace>`, which ejects the whole disk.
4. A final dialog: safe to unplug, or the error output.

In dry-run mode the flow runs up to step 3 but executes nothing that changes the system; the
last dialog lists the commands it *would* have run.

## The menu

Tauri builds the menu ([`tray.rs`](../src-tauri/src/tray.rs)) from plain text items. A few
native touches are added on top:

- **Styling right before the menu opens.** tray-icon opens the menu by calling
  `-[NSStatusItem setMenu:]` and then `performClick`. [`menu_style.rs`](../src-tauri/src/menu_style.rs)
  wraps that setter (only in this process) and styles the real `NSMenuItem`s by position:
  a bold header with a `subtitle` and a drive badge, a real `sectionHeader` for Time Machine,
  and indentation for extra lines.
- **SF Symbols on macOS 27.** macOS 27 hides menu item images unless
  `NSMenuItem.preferredImageVisibility` is `.visible`. Tauri never sets it, so
  [`menu_icons.rs`](../src-tauri/src/menu_icons.rs) wraps `-[NSMenuItem setImage:]` to set
  it. Tauri also only accepts full-color PNGs or a fixed list of system image names, so each
  symbol is drawn as an 18×18 template image and registered under the name of an obsolete
  system image the app never shows.
- **Keeping an open menu open.** In tray-icon, `set_icon` and `set_tooltip` re-measure the
  status item, which closes an open menu. Both are only called when their value changes, and
  the menu is only rebuilt when its content changes; backup progress is updated in place.

## The Detail window

- **Glass.** Native Liquid Glass (`NSGlassEffectView`, Regular) behind a transparent web
  view, with a vibrancy fallback on older macOS. A transparent web view needs Tauri's
  `macos-private-api` feature, which is fine because the app is not meant for the Mac App
  Store.
- **Opening.** The window is created hidden and shown once the page has rendered its first
  status (`detail_ready`, sent from a layout effect because `requestAnimationFrame` does not
  fire in hidden windows; a 1.5 s fallback shows it anyway). The first status already carries
  the storage numbers and the last known list of apps.
- **Closing.** Closing only hides the window, so reopening it within 2 minutes is instant.
  After that the web view and its web process are destroyed. A hidden window gets no events.
- **IPC.** The frontend only calls the commands in [`api.ts`](../src/api.ts) and listens for
  `status-changed`. Its capabilities are limited to listening for events and dragging the
  window; no shell or opener plugin is used.

The visual design is recorded in [`DESIGN.md`](../DESIGN.md) and the product context in
[`PRODUCT.md`](../PRODUCT.md).

## Running from the SSD itself

During development the app often runs from `target/` on the very SSD it manages. Two
consequences are handled explicitly:

- The process changes its working directory to `/` at startup, and every child process is
  started with `/` as its working directory, so the app never holds the SSD by accident.
- If that volume is force-ejected, the executable's code pages disappear. The Rust runtime's
  SIGBUS handler would then fault forever at 100 % CPU, so when running from `/Volumes/…`
  SIGBUS is routed straight to `_exit` instead (`system::exit_on_vanished_code`).
