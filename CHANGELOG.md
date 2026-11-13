# Changelog

All notable changes to this project are documented here. The format follows
[Keep a Changelog](https://keepachangelog.com/en/1.1.0/), and the project uses
[Semantic Versioning](https://semver.org/).

## [Unreleased]

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
