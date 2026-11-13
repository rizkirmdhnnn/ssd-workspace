---
version: 1
slug: "src-app-tsx"
primary_target: "src/App.tsx"
related_targets: []
---

# Detail Window

Scope: the Detail window (360×560, a Tauri webview on top of native glass). Mode: Operate.
Task: answer "can I eject the SSD right now?", show what is in the way and what will be handled automatically; then eject, or back up first.
Real content: apps holding the SSD (lsof processes grouped by their outermost .app bundle, with real app icons), Time Machine latest backup/progress, Herd, the APFS container split into Workspace, Backup, other volumes, and free space.
Constraints: every action goes through a Rust command; English text; light/dark follows the system; the verdict and its app count are shared with the tray menu header (src-tauri/src/menu_style.rs). Browse backups and Time Machine settings live in the tray menu, not here.

## Direction contract

THESIS: One SSD, one glass panel, one question. This window feels like a macOS 26 Control Center module that answers whether the SSD can be ejected, as an eject checklist, not a web page of cards or a dashboard of meters. It rejects the "white cards stacked on gray" arrangement.
OWN-WORLD: Native Liquid Glass Regular as the ground; on top of it thin transparent content platters (no double blur), concentric radii (window about 26, platter 16, capsule controls); round badges and round checklist glyphs (ok / auto / off) like system checklists. SF system font, tabular figures. Color: neutrals from the glass + the system accent for the primary action and the Workspace segment; green/orange/red only for status; Backup purple and an alpha neutral only as storage segments. SF Symbols-style SVG icons, uniform stroke.
STORY: The user reads the verdict in one second ("Ready to eject" / "Close 4 apps first"), scans the checklist for which apps to close (Show brings one to the front) and what will be stopped automatically, then presses Eject SSD… or Back Up.
FIRST VIEWPORT: Top: traffic lights floating on glass, in line with the "Dry run" pill when active and the refresh icon button; no title. Centered verdict hero: a 44px round badge with the drive icon tinted ok/warn/idle, a 17px semibold verdict, a 12px secondary sentence. One checklist platter, sized to its rows and scrolling with a bottom fade when long: a row per holding app (real app icon with an orange dot, name, "folder · N processes", Show) or "No apps using the SSD"; Time Machine (idle + latest backup + Back Up, or backing up + will be stopped + Skip + thin progress); Herd (not running, or will be stopped). Storage platter pinned to the bottom: stacked APFS bar with legend and "N GB free of N GB". Bottom: the "Open in Finder" capsule and the accent "Eject SSD…" capsule.
SIGNATURE: The verdict hero plus the eject checklist: one answer, counted in apps (the same count as the tray menu header), and below it every row marked done, handled automatically on eject, or not applicable.
RISK: Glass over a light wallpaper lowers the contrast of secondary text; guarded by filled platters and secondary text at ≥4.5:1.
seed: none (pinned)
