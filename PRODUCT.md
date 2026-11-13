# Product

<!-- impeccable:product-schema 1 -->

## Platform

web

(A web UI inside a Tauri 2 app for macOS 14+ only, run as a menu bar utility.)

## Users

Developers and other Mac users (typically on a laptop with macOS 26/27) who keep their work code and documents on an external SSD, often with Time Machine backing up to a second volume on the same disk. They glance at the menu bar to check the SSD and backup status, then eject the SSD before unplugging the cable (usually when done working or about to leave).

## Product Purpose

Monitor an external work SSD (by default the `/Volumes/Workspace` and `/Volumes/Backup` volumes in one APFS container; names are configurable) and eject it safely. It also replaces the built-in Time Machine icon in the menu bar, so the real Time Machine icon can be hidden. Success means the SSD is never unplugged dirty, backups are never damaged, and the menu bar stays tidy.

## Positioning

Combines SSD management and Time Machine in a single icon, with proven safety rules: always confirm, Herd is only touched when it is running, user processes are never killed.

## Operating Context

- The main interaction is the tray menu; the Detail window (about 360×560) is opened occasionally, mostly when the menu says the SSD is not ready to eject.
- Laravel Herd is only turned on during development.
- Time Machine backs up every hour to the Backup volume on the same SSD.
- This project itself lives on that SSD: testing the eject must use dry-run mode.
- The app runs all day on a laptop, often on battery: it must cost next to nothing while idle.

## Capabilities and Constraints

- Status: Workspace/Backup mounted, free space, Time Machine (latest backup, progress), Herd, apps using the SSD (processes grouped by app), APFS container usage.
- Actions: Open in Finder, Show (bring an app holding the SSD to the front), Eject (confirmation + Try Again), Back Up Now, Skip This Backup, Browse Backups, Time Machine Settings, Open at Login.
- All system logic is in Rust; the frontend only calls commands.
- `tmutil latestbackup` needs Full Disk Access; the backup date is read from the Time Machine preferences.
- Interface text is in English.

## Brand Commitments

- Must feel native to macOS 26+: Liquid Glass (Regular variant), system font, follows light/dark mode. The user's choice.
- A monochrome template tray icon; while a backup is running it carries a Time Machine badge.

## Product Principles

1. Data safety over speed: no destructive action without confirmation.
2. A glance is enough: the most important status is readable without opening anything.
3. Feels like part of macOS, not a web app.
4. Never touch something that is not running (Herd, a backup).
