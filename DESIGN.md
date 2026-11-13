---
name: SSD Workspace
description: A macOS menu bar utility to monitor, back up, and safely eject the Workspace SSD.
colors:
  accent: "#0a7aff"
  on-accent: "#ffffff"
  link: "#0068da"
  link-dark: "#419cff"
  ok: "#1a7f37"
  ok-dark: "#32d74b"
  warn: "#a35200"
  warn-dark: "#ffb340"
  danger: "#d70015"
  danger-dark: "#ff6961"
  label: "rgba(0, 0, 0, 0.86)"
  label-dark: "rgba(255, 255, 255, 0.9)"
  secondary: "rgba(0, 0, 0, 0.58)"
  secondary-dark: "rgba(255, 255, 255, 0.62)"
  tertiary: "rgba(0, 0, 0, 0.32)"
  tertiary-dark: "rgba(255, 255, 255, 0.3)"
  platter: "rgba(255, 255, 255, 0.22)"
  platter-dark: "rgba(255, 255, 255, 0.07)"
  platter-edge: "rgba(0, 0, 0, 0.06)"
  platter-edge-dark: "rgba(255, 255, 255, 0.09)"
  control: "rgba(0, 0, 0, 0.05)"
  control-dark: "rgba(255, 255, 255, 0.12)"
  control-hover: "rgba(0, 0, 0, 0.09)"
  control-hover-dark: "rgba(255, 255, 255, 0.2)"
  control-edge: "rgba(0, 0, 0, 0.06)"
  control-edge-dark: "rgba(255, 255, 255, 0.1)"
  track: "rgba(0, 0, 0, 0.09)"
  track-dark: "rgba(255, 255, 255, 0.12)"
  hairline: "rgba(0, 0, 0, 0.07)"
  hairline-dark: "rgba(255, 255, 255, 0.08)"
  badge: "rgba(0, 0, 0, 0.06)"
  badge-dark: "rgba(255, 255, 255, 0.1)"
  on-ok: "#ffffff"
  seg-backup: "#a550d3"
  seg-backup-dark: "#c37ef0"
  seg-other: "rgba(0, 0, 0, 0.22)"
  seg-other-dark: "rgba(255, 255, 255, 0.28)"
typography:
  headline:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "17px"
    fontWeight: 600
    lineHeight: 1.35
    letterSpacing: "-0.01em"
  title:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 600
    lineHeight: 1.35
  body:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 400
    lineHeight: 1.35
  control:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "13px"
    fontWeight: 500
    lineHeight: 1.35
  footnote:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "12px"
    fontWeight: 400
    lineHeight: 1.35
  caption:
    fontFamily: "-apple-system, BlinkMacSystemFont, system-ui, sans-serif"
    fontSize: "11px"
    fontWeight: 500
    lineHeight: 1.35
rounded:
  platter: "16px"
  control: "999px"
  badge: "50%"
spacing:
  hairline-gap: "1px"
  xs: "6px"
  sm: "8px"
  md: "10px"
  inset: "12px"
  lg: "14px"
  indent: "32px"
components:
  platter:
    backgroundColor: "{colors.platter}"
    rounded: "{rounded.platter}"
    padding: "12px"
  capsule:
    backgroundColor: "{colors.control}"
    textColor: "{colors.label}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "30px"
  capsule-hover:
    backgroundColor: "{colors.control-hover}"
  capsule-primary:
    backgroundColor: "{colors.accent}"
    textColor: "{colors.on-accent}"
    typography: "{typography.control}"
    rounded: "{rounded.control}"
    padding: "0 14px"
    height: "30px"
  capsule-small:
    backgroundColor: "{colors.control}"
    textColor: "{colors.label}"
    rounded: "{rounded.control}"
    padding: "0 12px"
    height: "26px"
  icon-button:
    textColor: "{colors.secondary}"
    rounded: "{rounded.control}"
    size: "28px"
  icon-button-hover:
    backgroundColor: "{colors.control}"
    textColor: "{colors.label}"
  hero-badge:
    backgroundColor: "{colors.badge}"
    textColor: "{colors.secondary}"
    rounded: "{rounded.badge}"
    size: "44px"
  glyph-ok:
    backgroundColor: "{colors.ok}"
    textColor: "{colors.on-ok}"
    rounded: "{rounded.badge}"
    size: "22px"
  glyph-auto:
    backgroundColor: "{colors.badge}"
    textColor: "{colors.secondary}"
    rounded: "{rounded.badge}"
    size: "22px"
  glyph-off:
    backgroundColor: "{colors.badge}"
    textColor: "{colors.tertiary}"
    rounded: "{rounded.badge}"
    size: "22px"
  app-icon:
    size: "22px"
  holder-dot:
    backgroundColor: "{colors.warn}"
    rounded: "{rounded.badge}"
    size: "9px"
  link:
    textColor: "{colors.link}"
    typography: "{typography.footnote}"
  pill-status:
    textColor: "{colors.warn}"
    typography: "{typography.caption}"
    rounded: "{rounded.control}"
    padding: "2px 8px"
  checklist-platter:
    backgroundColor: "{colors.platter}"
    rounded: "{rounded.platter}"
    padding: "4px 12px"
  meter-thin:
    backgroundColor: "{colors.track}"
    rounded: "{rounded.control}"
    height: "4px"
  meter-stacked:
    backgroundColor: "{colors.track}"
    rounded: "{rounded.control}"
    height: "8px"
  legend-dot:
    rounded: "{rounded.badge}"
    size: "8px"
---

# Design System: SSD Workspace

## Overview

**Creative North Star: "One Glass Panel"**

The Detail window is a single macOS 26 Control Center module, not a web page of cards. Its ground is native Liquid Glass (Regular variant) that Tauri installs behind a transparent webview; the page only lays thin, transparent content platters on top of that glass, with no second blur. Every neutral color is derived from the glass through alpha labels, so the panel turns light or dark with the system and tints itself from the wallpaper behind it.

The window answers one question, "can I eject the SSD right now?", and reads as an eject checklist: a centered verdict at the top, then one platter with a row per thing that stands between the user and a safe eject (each app holding the SSD, Time Machine, Herd), then the storage summary and the actions. Its density is the density of a system utility: a fixed 360×560 window, 13px text, 30px capsule controls, one column. One 17px verdict title is the only large text; everything else is 11–13px and ranked by weight and label alpha. Color has three jobs: the system accent for the primary action and progress, green/orange/red for status, and two quiet segment colors that tell the volumes apart in the storage bar.

The corners are concentric: the native window corner (about 26px) hugs 16px platters that sit 12px from the edge, and every control is a full capsule; badges and glyphs are circles. Icons are SF Symbols-style lines on a 20×20 grid with a uniform 1.6 stroke.

**Key Characteristics:**
- Native Liquid Glass as the only blurred surface; the platters on top of it are transparent.
- Alpha neutrals (label/secondary/tertiary) read on top of glass, with complete light/dark pairs.
- Concentric radii: window ~26, platter 16, capsule controls 999px, round badges and glyphs.
- One verdict, shared with the tray menu header, counted in apps rather than processes.
- SF system font at 11–13px plus one 17px verdict title; tabular figures for every value.
- System accent (AccentColor) only for the primary action, the Workspace segment, and progress.
- 20×20 line icons, 1.6 stroke, rounded caps and joins.

## Colors

An alpha neutral palette that rides on the glass, one system accent, one native link color, three status colors, and two storage segment colors.

### Primary
- **System Accent** (`AccentColor`, fallback {colors.accent}): the accent color the user picked in System Settings, read through CSS `AccentColor` / `AccentColorText` when available. Used for the primary action capsule ("Eject SSD…"), the Workspace segment of the storage bar and its legend dot, the Time Machine progress meter, the focus ring (70% accent), and text selection (30% accent). Hovering the primary capsule mixes 88% accent with white.

### Secondary
- **Native Link Blue** ({colors.link} light, {colors.link-dark} dark): the value of `NSColor.linkColor`, deliberately not CSS `LinkText` (which in WebKit gives the classic web blue/purple). Used only for 12px text links; today that is the trailing "Show" on an app row, which only brings that app to the front.

### Tertiary (status)
- **Ready Green** ({colors.ok} / {colors.ok-dark}): the verdict badge when the SSD is ready to eject (green icon on 16% green), and the filled circle of an "ok" checklist glyph with a white ({colors.on-ok}) check.
- **Attention Orange** ({colors.warn} / {colors.warn-dark}): the verdict badge while apps still hold the SSD or an eject is running (orange icon on 16% orange), the 9px dot on each holding app's icon, and the "Dry run" pill (orange text on 16% orange).
- **Error Red** ({colors.danger} / {colors.danger-dark}): the 12px error message above the footer. Not used for buttons.

### Storage segments
- **Backup Purple** ({colors.seg-backup} / {colors.seg-backup-dark}): the Backup volume's segment in the storage bar and its legend dot. Nowhere else.
- **Other Volumes** ({colors.seg-other} / {colors.seg-other-dark}): an alpha neutral segment for any other volume in the APFS container (drawn only above 1 GB); it has no legend entry.

### Neutral
- **Label** ({colors.label} / {colors.label-dark}): primary text.
- **Secondary Label** ({colors.secondary} / {colors.secondary-dark}): the verdict sentence, checklist sublines, storage values and legend, the refresh button icon, the idle verdict badge icon, the stop square of an "auto" glyph, and the fallback app icon.
- **Tertiary Label** ({colors.tertiary} / {colors.tertiary-dark}): inactive states only: the dash of an "off" glyph (Time Machine not set up).
- **Platter** ({colors.platter} / {colors.platter-dark}) with **Platter Edge** ({colors.platter-edge} / {colors.platter-edge-dark}) as a 0.5px inner line.
- **Control** ({colors.control} → hover {colors.control-hover}) with **Control Edge** ({colors.control-edge}); the dark counterparts are lighter because dark glass needs a thicker fill.
- **Track** ({colors.track}): the meter rail, and the free space of the storage bar. **Hairline** ({colors.hairline}): 0.5px separators. **Badge** ({colors.badge}): the fill of the idle verdict badge and of "auto"/"off" glyphs.

### Named Rules
**The Glass-Derived Neutral Rule.** Every neutral is black or white with alpha, never a solid gray; the glass behind it supplies the color. Every neutral token must have a dark pair.

**The Status-Only Hue Rule.** Green, orange, and red only state status. The primary action uses the System Accent; there are no green or red buttons.

**The Segment Hue Rule.** Backup Purple and Other Volumes exist only to tell volumes apart inside the storage bar and its legend; they never mark status, and never fill a control.

**The Native Link Rule.** Links use the value of `NSColor.linkColor`, not web blue; it rejects the "visited" purple and is only underlined on hover.

## Typography

**Display Font:** none; this system does not use a display face.
**Body Font:** SF Pro through `-apple-system` (fallback `BlinkMacSystemFont`, `system-ui`, sans-serif)

**Character:** The macOS system font at control size (13px), antialiased, not selectable, default cursor: interface text, not document text. Every number (GB, process count, percent, time) uses tabular figures.

### Hierarchy
- **Headline** (600, 17px, 1.35, -0.01em): the verdict title only: "Ready to eject", "Close 4 apps first", "Not mounted", "Checking…", "Ejecting…". One per window.
- **Title** (600, 13px, 1.35): the "Storage" platter label.
- **Body** (400, 13px, 1.35): checklist row titles (app names, "Time Machine idle", "Herd not running"), truncated with an ellipsis.
- **Control** (500, 13px): capsule labels; small capsules drop to 12px.
- **Footnote** (400, 12px): the verdict sentence, checklist sublines ("folder · N processes", "Latest backup: …"), storage values and legend, links, error messages.
- **Caption** (500, 11px): the "Dry run" status pill.

### Named Rules
**The Weight-Not-Size Rule.** Apart from the one 17px verdict title, the size range is only 11–13px. Hierarchy is built with weight (600/500/400) and label alpha, never with another large heading.

**The Tabular Rule.** Every number that can change uses `font-variant-numeric: tabular-nums` so rows do not shift when the status updates.

## Layout

One fixed column in a 360×560 window that cannot be resized; there are no breakpoints. The column has a 12px inset on the left, right, and bottom; the top is a draggable 46px titlebar area, with the native traffic lights at (18, 25) and the titlebar content (the "Dry run" pill, then the refresh button pushed right) starting at 76px to line up with them. There is no window title on the page.

Top to bottom, 8px apart: titlebar → verdict hero (centered, pulled up 10px under the titlebar, 6px below; a 44px badge, 6px gap, the 17px title, the 12px sentence, 2px apart; also a drag region) → checklist platter → storage platter → error line (when present) → footer. The checklist platter is as tall as its rows and only shrinks, then scrolls, when the window runs out of room; its bottom edge then fades over 14px through a mask. The storage platter is pinned to the bottom with an auto top margin. The footer holds the neutral capsule on the left and the primary capsule pushed right.

Checklist rows: 7px vertical padding, a 22px leading glyph or app icon, a 10px gap, a text stack (title over a 12px subline, 1px apart), and an optional trailing link or small capsule. Content below a row (the Time Machine progress meter) sits 6px under it and is indented 32px, the glyph width plus its gap, so it lines up with the text column. When the SSD is not mounted, the checklist and storage platters are not rendered; only the verdict and the disabled footer remain.

**The Glyph-Column Rule.** Everything that belongs to a checklist row but is not its glyph (sub-content, hairline separators) starts at 32px, the text column; it never starts at the platter edge.

## Elevation & Depth

Depth comes from material, not shadows. The only blurred layer is the window's native Liquid Glass (`Effect::LiquidGlassRegular`, fallback `UnderWindowBackground` on older macOS, active following the window state). Platters and controls are transparent fills bounded by a 0.5px inner line; there are no drop shadows on the page.

### Shadow Vocabulary
- **Platter edge** (`box-shadow: inset 0 0 0 0.5px var(--platter-edge)`): the platter boundary on top of glass.
- **Control edge** (`box-shadow: inset 0 0 0 0.5px var(--control-edge)`): the boundary of a neutral capsule; the primary capsule does not use it.

### Named Rules
**The Single Blur Rule.** The native glass is the only blur. The webview stays transparent and never adds a `backdrop-filter` on top of it.

**The Hairline Edge Rule.** A surface boundary is a 0.5px inner line with alpha, not a solid border and not a drop shadow.

## Shapes

The corner language is concentric and fully rounded. The native window corner (about 26px) hugs 16px platters at a 12px distance. Every pressable control, pill, and meter is a capsule (999px). Round shapes carry state: the 44px verdict badge, 22px checklist glyphs, the 9px holder dot on an app icon, and 8px legend dots. Separators are 0.5px hairlines between checklist rows, starting at 32px like macOS table separators. The storage bar is one 8px capsule split into square-edged segments with 1.5px gaps; only the last segment keeps the capsule's right end.

Checklist glyphs have three kinds: **ok** (green filled circle, white check), **auto** (Badge-filled circle, Secondary stop square: will be handled automatically on eject), and **off** (Badge-filled circle, Tertiary dash: not applicable). Holding apps use their real app icon at 22px instead of a glyph, with the orange holder dot at the lower right.

Icons: SF Symbols-style lines on a 20×20 viewBox, 1.6 stroke, rounded caps and joins, `currentColor`, no fill except glyphs that are solid by nature (Eject, the stop square). Small dots inside an icon (drive LEDs) use a 2.2 stroke; the check and dash glyphs use 2.4 so they hold at 14px. Display sizes: 22px in the verdict badge, 15px in a capsule, 16px in an icon button, 14px inside a checklist glyph, 18px for the fallback app icon. The tray icons are separate monochrome template images (`src-tauri/icons/tray/`).

**The Concentric Rule.** Inner radius = outer radius minus the distance between them; elements inside a platter never use a corner larger than the platter's, except capsules and circles, which are fully rounded by nature.

## Components

### Buttons
Calm capsules that feel like Control Center controls.
- **Shape:** full capsule (999px), 30px tall, padding 0 14px, 15px icon with a 6px gap.
- **Neutral capsule:** Control fill + 0.5px control edge, 500 label. Hover goes to Control Hover; pressed shrinks to 0.97. 0.2s transition with `--ease-out`. In the footer: "Open in Finder".
- **Primary capsule:** System Accent fill, `AccentColorText` text, no edge line; hover mixes 88% accent with white. Only one per window ("Eject SSD…", "Ejecting…" while busy).
- **Small capsule:** 26px tall, padding 0 12px, 12px; the trailing action of a checklist row ("Back Up" when Time Machine is idle, "Skip" while it backs up).
- **Icon button:** a transparent 28px circle, Secondary Label icon; hover gets a Control fill and a Label icon. The refresh icon spins while checking.
- **Focus:** 2px outline at 70% accent, 2px offset. **Disabled:** 0.45 opacity.

### Links
- **Style:** 12px Native Link Blue text, no background or padding; underlined only on hover with a 2px offset. Used as the trailing "Show" of an app row; a link never ejects, quits, or kills anything.

### Platter
- **Corner Style:** 16px.
- **Background:** Platter + 0.5px inner Platter Edge.
- **Internal Padding:** 12px, content stacked with an 8px gap (storage platter); the checklist platter uses 4px 12px so its rows supply the vertical rhythm.

### Status Pill
- An 11px/500 capsule, padding 2px 8px, status-color text on 16% status color. Currently only "Dry run" in the titlebar.

### Verdict Hero (signature)
The window's one answer, centered under the titlebar: a 44px round badge with the drive icon, a 17px/600 title, and a 12px Secondary sentence. The badge tone follows the verdict: **ok** (green on 16% green) "Ready to eject", with the sentence naming what will be stopped first; **warn** (orange on 16% orange) "Close N apps first" or "Ejecting…"; **idle** (Secondary on Badge) "Checking…" or "Not mounted". Badge color and fill change over 0.4s. The tray menu header states the same verdict from the same count, in apps.

### Eject Checklist (signature)
One platter, one row per thing between the user and a safe eject, in this order:
- **Apps holding the SSD:** one row per app (processes grouped by their outermost `.app` bundle), real 22px app icon with the orange holder dot, the app name, the subline "folder · N processes" (count shown only above one), and a trailing "Show" link. When none: an ok glyph and "No apps using the SSD".
- **Time Machine:** idle: ok glyph, "Time Machine idle", "Latest backup: …", small "Back Up" capsule (disabled when the Backup volume is not mounted). Backing up: auto glyph, "Time Machine is backing up", the status text, percent while copying, and "· will be stopped", a small "Skip" capsule, and a 4px progress meter indented 32px (indeterminate 30% sliding segment without a percent). Not set up: off glyph.
- **Herd:** ok glyph "Herd not running", or auto glyph "Herd is running" / "Will be stopped".
- Rows are separated by 0.5px hairlines starting at 32px.

### Storage Bar
- The "Storage" title with "N GB free of N GB" right-aligned in Secondary, then an 8px stacked meter of the APFS container: Workspace (accent), Backup (Backup Purple), other volumes (Other Volumes), free space left as Track. Below it a 12px legend with 8px dots for Workspace and Backup and their sizes. Segment widths move over 0.7s.

## Do's and Don'ts

### Do:
- **Do** let native Liquid Glass be the only blurred surface and put content on transparent 16px platters.
- **Do** give every color token a dark pair and build neutrals from black/white with alpha.
- **Do** use capsules (999px) for every control and circles for badges, glyphs, and dots.
- **Do** use the System Accent (`AccentColor`) for one primary action per window, the Workspace segment, and progress.
- **Do** state eject readiness once, as the verdict, and count it in apps, the same way the tray menu header does.
- **Do** mark each checklist row with one of the three glyph kinds (ok, auto, off), or a real app icon with the holder dot.
- **Do** use the value of `NSColor.linkColor` for links and tabular figures for every value.
- **Do** draw new icons on the 20×20 grid, 1.6 stroke, rounded caps, `currentColor`.
- **Do** turn off every animation and transition under `prefers-reduced-motion: reduce`.

### Don't:
- **Don't** add `backdrop-filter` or a second blur in the webview on top of the native glass.
- **Don't** stack solid white cards on a gray background; platters are always transparent.
- **Don't** use green, orange, or red for buttons; those colors are status only.
- **Don't** use the segment colors outside the storage bar and its legend.
- **Don't** use CSS `LinkText` or web blue/purple for links.
- **Don't** use drop shadows for depth; a surface boundary is a 0.5px inner line.
- **Don't** write a window title on the page; the titlebar only holds the traffic lights, the status pill, and the refresh button, and the verdict carries the window.
- **Don't** list raw processes or PIDs; holders are shown as apps.
- **Don't** use Tertiary Label for text that has to be read; it is for inactive states only.
