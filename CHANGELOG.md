# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

## [0.1.1] - 2026-11-14

### Added

- Backup progress in the menu, like the built-in Time Machine menu: status, a progress bar
  and the time remaining, updated live while the menu is open.
- The menu header shows the backup percentage ("Backing up 42% · 770 GB free").
- After Skip This Backup, the menu and the Detail window say "Stopping backup…" until Time
  Machine has stopped, and Skip is disabled meanwhile.

### Fixed

- The menu and the Detail window no longer say "Ready to eject" while Time Machine is backing
  up to the SSD; they say "Backing up" instead.

### Changed

- Eject is disabled while Time Machine backs up to the SSD, so a backup is never cut short.
  Wait for it to finish or choose Skip This Backup first.

## [0.1.0] - 2026-11-13

First public release.

### Added

- Menu bar app with a template icon for mounted, not mounted, backing up and ejecting.
- Menu header with eject readiness ("Ready to eject" / "N apps using the SSD") and free space.
- Safe eject flow: confirmation, stops a running Time Machine backup and running Herd
  services, lists what still holds the disk with Try Again, never kills user processes.
- Time Machine section replacing the built-in menu: latest backup, Back Up Now, live
  progress, Skip This Backup, Browse Backups, Time Machine Settings.
- Detail window (Liquid Glass) with a verdict, a checklist of the apps holding the SSD
  (grouped by app, with icons and Show), Time Machine and Herd, and an APFS storage bar.
- `config.json` to choose the workspace and backup volume names.
- Open at Login, dry-run mode and state simulation for development, `SSD_WORKSPACE_TRACE`.

[Unreleased]: https://github.com/rizkirmdhnnn/ssd-workspace/compare/v0.1.1...HEAD
[0.1.1]: https://github.com/rizkirmdhnnn/ssd-workspace/compare/v0.1.0...v0.1.1
[0.1.0]: https://github.com/rizkirmdhnnn/ssd-workspace/releases/tag/v0.1.0
