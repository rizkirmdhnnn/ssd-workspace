//! Menu bar icon and menu, including the Time Machine section that mirrors the built-in
//! menu (so the real Time Machine icon can be hidden).

use crate::menu_icons::{self, Icon};
use crate::menu_style::{self, Role};
use crate::{Snapshot, system};
use std::sync::Mutex;
use tauri::image::Image;
use tauri::menu::{
    CheckMenuItem, IconMenuItem, IsMenuItem, Menu, MenuEvent, MenuItem, PredefinedMenuItem, Submenu,
};
use tauri::tray::TrayIconBuilder;
use tauri::{AppHandle, Wry};

const TRAY_ID: &str = "main";

const ICON_MOUNTED: &[u8] = include_bytes!("../icons/tray/tray-mounted.png");
const ICON_UNMOUNTED: &[u8] = include_bytes!("../icons/tray/tray-unmounted.png");
const ICON_BACKUP: &[u8] = include_bytes!("../icons/tray/tray-backup.png");
const ICON_EJECTING: &[u8] = include_bytes!("../icons/tray/tray-ejecting.png");

mod id {
    pub const OPEN: &str = "open";
    pub const EJECT: &str = "eject";
    pub const DETAIL: &str = "detail";
    pub const HEADER: &str = "header";
    pub const TM_START: &str = "tm-start";
    pub const TM_SKIP: &str = "tm-skip";
    pub const TM_BROWSE: &str = "tm-browse";
    pub const TM_SETTINGS: &str = "tm-settings";
    pub const REFRESH: &str = "refresh";
    pub const AUTOSTART: &str = "autostart";
    pub const DRY_RUN: &str = "dry-run";
    pub const QUIT: &str = "quit";
}

/// The menu that was installed last: its skeleton (without progress) plus the Time Machine
/// progress line, so progress can be updated in place without rebuilding an open menu.
///
/// macOS note: `set_icon` and `set_tooltip` in tray-icon re-measure the status button, and
/// that CLOSES an open menu. Both are only called when their value actually changes.
struct Built {
    skeleton: Snapshot,
    tm_line: Option<MenuItem<Wry>>,
    tooltip: String,
    icon: &'static [u8],
}

static BUILT: Mutex<Option<Built>> = Mutex::new(None);

/// Only what the menu shows: progress is cleared and space is rounded to GB
/// (raw bytes keep changing during a backup).
fn skeleton(s: &Snapshot) -> Snapshot {
    let mut k = s.clone();
    if k.tm.running {
        k.tm.progress = None;
        k.tm.status_text.clear();
    }
    k.workspace_space = k.workspace_space.map(|sp| system::Space {
        free: (sp.free as f64 / 1e9).round() as u64,
        total: (sp.total as f64 / 1e9).round() as u64,
    });
    k
}

fn icon_bytes(s: &Snapshot) -> &'static [u8] {
    if s.busy {
        ICON_EJECTING
    } else if s.tm.running {
        ICON_BACKUP
    } else if s.workspace_mounted {
        ICON_MOUNTED
    } else {
        ICON_UNMOUNTED
    }
}

fn icon_for(s: &Snapshot) -> Image<'static> {
    Image::from_bytes(icon_bytes(s)).expect("invalid tray icon")
}

fn tooltip_for(s: &Snapshot) -> String {
    if s.busy {
        "SSD Workspace: ejecting…".into()
    } else if s.tm.running {
        // Deliberately without progress: a tooltip that changes every tick makes tray-icon
        // re-measure the icon, and that closes an open menu.
        "SSD Workspace: Time Machine is backing up".into()
    } else if s.workspace_mounted {
        "SSD Workspace: mounted".into()
    } else {
        "SSD Workspace: not mounted".into()
    }
}

type BuiltMenu = (Menu<Wry>, Option<MenuItem<Wry>>, Vec<(usize, Role)>);

fn build_menu(app: &AppHandle, s: &Snapshot) -> tauri::Result<BuiltMenu> {
    let info = |text: &str| MenuItem::new(app, text, false, None::<&str>);
    let item = |id: &str, text: &str, enabled: bool, accel: Option<&str>| {
        MenuItem::with_id(app, id, text, enabled, accel)
    };
    let action = |id: &str, icon: Icon, text: &str, enabled: bool, accel: Option<&str>| {
        IconMenuItem::with_id_and_native_icon(
            app,
            id,
            text,
            enabled,
            menu_icons::native(icon),
            accel,
        )
    };
    let sep = || PredefinedMenuItem::separator(app);

    let mut items: Vec<Box<dyn IsMenuItem<Wry>>> = Vec::new();
    // Native styling applied when the menu opens (see menu_style.rs), by top-level index.
    let mut layout: Vec<(usize, Role)> = Vec::new();
    macro_rules! styled {
        ($role:expr, $item:expr) => {{
            layout.push((items.len(), $role));
            items.push(Box::new($item));
        }};
    }

    // --- Header: one glance at the SSD (subtitle, badge and readiness added on open) ---
    styled!(Role::Header, item(id::HEADER, "SSD Workspace", true, None)?);
    if let Some(backup) = crate::config::get().backup.as_deref()
        && s.workspace_mounted
        && !s.backup_mounted
    {
        styled!(
            Role::Indent,
            info(&format!("“{backup}” volume not mounted"))?
        );
    }
    if s.dry_run {
        styled!(Role::Indent, info("Dry run: eject is only simulated")?);
    }

    // --- Main actions ---
    if s.workspace_mounted {
        items.push(Box::new(sep()?));
        items.push(Box::new(action(
            id::EJECT,
            Icon::Eject,
            if s.busy {
                "Ejecting…"
            } else {
                "Eject SSD…"
            },
            !s.busy,
            Some("CmdOrCtrl+E"),
        )?));
        items.push(Box::new(action(
            id::OPEN,
            Icon::Folder,
            "Open in Finder",
            true,
            None,
        )?));
    }
    items.push(Box::new(sep()?));

    // --- Time Machine (replaces the built-in menu) ---
    let tm = &s.tm;
    let mut tm_line = None;
    styled!(Role::Section, info("Time Machine")?);
    if !tm.configured {
        items.push(Box::new(info("Not set up")?));
    } else if tm.running {
        let line = info(&tm.status_text)?;
        tm_line = Some(line.clone());
        items.push(Box::new(line));
        items.push(Box::new(action(
            id::TM_SKIP,
            Icon::Skip,
            "Skip This Backup",
            true,
            None,
        )?));
    } else {
        let latest = tm.latest_text.as_deref().unwrap_or("no backups yet");
        items.push(Box::new(info(&format!("Latest: {latest}"))?));
        items.push(Box::new(action(
            id::TM_START,
            Icon::BackUp,
            "Back Up Now",
            // With no backup volume configured, Time Machine backs up somewhere else.
            s.backup_mounted || crate::config::get().backup.is_none(),
            None,
        )?));
    }
    if tm.configured {
        items.push(Box::new(item(
            id::TM_BROWSE,
            "Browse Backups…",
            true,
            None,
        )?));
    }
    items.push(Box::new(sep()?));

    // --- App ---
    items.push(Box::new(action(
        id::DETAIL,
        Icon::Details,
        "Show Details…",
        true,
        Some("CmdOrCtrl+D"),
    )?));

    let mut options: Vec<Box<dyn IsMenuItem<Wry>>> = vec![
        Box::new(item(
            id::REFRESH,
            "Refresh Status",
            true,
            Some("CmdOrCtrl+R"),
        )?),
        Box::new(CheckMenuItem::with_id(
            app,
            id::AUTOSTART,
            "Open at Login",
            true,
            s.autostart,
            None::<&str>,
        )?),
        Box::new(sep()?),
        Box::new(item(id::TM_SETTINGS, "Time Machine Settings…", true, None)?),
    ];
    if cfg!(debug_assertions) {
        options.push(Box::new(sep()?));
        options.push(Box::new(CheckMenuItem::with_id(
            app,
            id::DRY_RUN,
            "Dry Run Mode",
            true,
            s.dry_run,
            None::<&str>,
        )?));
    }
    let option_refs: Vec<&dyn IsMenuItem<Wry>> = options.iter().map(|b| b.as_ref()).collect();
    let submenu = Submenu::with_id_and_native_icon(
        app,
        "options",
        "Options",
        true,
        menu_icons::native(Icon::Options),
    )?;
    submenu.append_items(&option_refs)?;
    items.push(Box::new(submenu));
    items.push(Box::new(item(id::QUIT, "Quit", true, Some("CmdOrCtrl+Q"))?));

    let refs: Vec<&dyn IsMenuItem<Wry>> = items.iter().map(|b| b.as_ref()).collect();
    Ok((Menu::with_items(app, &refs)?, tm_line, layout))
}

fn on_menu_event(app: &AppHandle, event: MenuEvent) {
    match event.id().as_ref() {
        id::OPEN => system::open_in_finder(system::workspace_path()),
        id::EJECT => crate::start_eject(app),
        id::DETAIL | id::HEADER => crate::show_detail(app),
        id::TM_START => crate::tm_start_backup(app),
        id::TM_SKIP => crate::tm_skip_backup(app),
        id::TM_BROWSE => crate::timemachine::browse(),
        id::TM_SETTINGS => crate::timemachine::open_settings(),
        id::REFRESH => crate::request_refresh(app),
        id::AUTOSTART => crate::toggle_autostart(app),
        id::DRY_RUN => crate::toggle_dry_run(app),
        id::QUIT => app.exit(0),
        _ => {}
    }
}

pub fn create(app: &AppHandle) -> tauri::Result<()> {
    menu_icons::register();
    menu_style::init(app);
    let initial = Snapshot {
        workspace_mounted: system::is_mounted(system::workspace_path()),
        ..Default::default()
    };
    let (menu, _, layout) = build_menu(app, &initial)?;
    menu_style::set_status(&initial);
    menu_style::set_layout(layout);
    TrayIconBuilder::with_id(TRAY_ID)
        .icon(icon_for(&initial))
        .icon_as_template(true)
        .tooltip(tooltip_for(&initial))
        .menu(&menu)
        .show_menu_on_left_click(true)
        .on_menu_event(on_menu_event)
        .build(app)?;
    Ok(())
}

/// Update the icon, tooltip, and menu to match the latest status. When only the backup
/// progress changed, the progress line is updated in place (visible right away in an open menu).
pub fn update(app: &AppHandle, s: &Snapshot) {
    let Some(tray) = app.tray_by_id(TRAY_ID) else {
        return;
    };
    let tooltip = tooltip_for(s);
    let mut built = BUILT.lock().unwrap();
    // Free space and readiness live in the header subtitle, filled in when the menu opens.
    menu_style::set_status(s);
    let sk = skeleton(s);
    if let Some(b) = built.as_ref().filter(|b| b.skeleton == sk) {
        if let Some(line) = &b.tm_line {
            let _ = line.set_text(&s.tm.status_text);
        }
        return;
    }

    if built.as_ref().is_none_or(|b| b.tooltip != tooltip) {
        let _ = tray.set_tooltip(Some(&tooltip));
    }
    let icon = icon_bytes(s);
    if built.as_ref().is_none_or(|b| b.icon != icon) {
        let _ = tray.set_icon(Some(icon_for(s)));
        let _ = tray.set_icon_as_template(true);
    }
    match build_menu(app, s) {
        Ok((menu, tm_line, layout)) => {
            menu_style::set_layout(layout);
            let _ = tray.set_menu(Some(menu));
            *built = Some(Built {
                skeleton: sk,
                tm_line,
                tooltip,
                icon,
            });
        }
        Err(e) => eprintln!("[ssd-workspace] failed to build the menu: {e}"),
    }
}
