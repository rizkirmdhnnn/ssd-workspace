//! Native styling Tauri cannot express: applied to the real `NSMenuItem`s right before the
//! tray menu opens.
//!
//! tray-icon opens the menu with `-[NSStatusItem setMenu:]` + `performClick`, so that
//! setter is wrapped (this process only). `tray::build_menu` records which top-level
//! positions get which role:
//!
//! - **Header**: bold "SSD Workspace", a `subtitle` ("Ready to eject · 776 GB free") and a
//!   drive badge with a status dot. Clicking it opens the Detail window.
//! - **Section**: replaced by a real `sectionHeader` item (macOS 14+).
//! - **Indent**: extra header lines, indented to line up with the item titles.
//!
//! Eject readiness needs `lsof`, which is too costly for the regular status refresh. It
//! runs in the background when the menu opens (at most every few seconds); the header
//! shows the latest result and is updated in place if the menu is still open.

use crate::{Snapshot, system};
use block2::RcBlock;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{MainThreadMarker, sel};
use objc2_app_kit::{
    NSBezierPath, NSColor, NSFont, NSFontAttributeName, NSImage, NSImageSymbolConfiguration,
    NSMenu, NSMenuItem,
};
use objc2_foundation::{NSAttributedString, NSDictionary, NSPoint, NSRect, NSSize, NSString};
use std::cell::RefCell;
use std::sync::{Mutex, OnceLock};
use std::time::{Duration, Instant};
use tauri::AppHandle;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Role {
    Header,
    Section,
    Indent,
}

/// Roles of the installed menu's top-level items, by index.
static LAYOUT: Mutex<Vec<(usize, Role)>> = Mutex::new(Vec::new());
/// The status the installed menu was built from.
static SNAPSHOT: Mutex<Option<Snapshot>> = Mutex::new(None);
static APP: OnceLock<AppHandle> = OnceLock::new();

/// Do not run `lsof` again within this time after the previous check finished.
const BUSY_MIN_INTERVAL: Duration = Duration::from_secs(5);

#[derive(Default)]
struct Busy {
    /// Apps holding the SSD at the last check, `None` before the first one.
    count: Option<usize>,
    finished: Option<Instant>,
    running: bool,
}

static BUSY: Mutex<Busy> = Mutex::new(Busy {
    count: None,
    finished: None,
    running: false,
});

thread_local! {
    /// The header item of the menu that is (or was last) open; main thread only.
    static HEADER: RefCell<Option<Retained<NSMenuItem>>> = const { RefCell::new(None) };
}

/// Install the hook. Call once on the main thread before the tray is created.
pub fn init(app: &AppHandle) {
    let _ = APP.set(app.clone());
    hook_status_item_set_menu();
}

/// Record the item roles of the menu that is about to be installed.
pub fn set_layout(layout: Vec<(usize, Role)>) {
    *LAYOUT.lock().unwrap() = layout;
}

/// Record the latest status (shown in the header the next time the menu opens).
pub fn set_status(s: &Snapshot) {
    let mut cur = SNAPSHOT.lock().unwrap();
    // A different SSD state makes the last process check meaningless.
    if cur
        .as_ref()
        .is_none_or(|c| c.workspace_mounted != s.workspace_mounted || c.busy != s.busy)
    {
        let mut b = BUSY.lock().unwrap();
        b.count = None;
        b.finished = None;
        crate::holders::invalidate();
    }
    *cur = Some(s.clone());
}

// ---------------------------------------------------------------------------------------
// Header content

#[derive(Clone, Copy, PartialEq)]
enum Tone {
    None,
    Ok,
    Warn,
}

fn header_content() -> (String, Tone) {
    let snap = SNAPSHOT.lock().unwrap().clone().unwrap_or_default();
    if !snap.workspace_mounted {
        return ("Not mounted · plug in the SSD".into(), Tone::None);
    }
    if snap.busy {
        return ("Ejecting…".into(), Tone::Warn);
    }
    let (state, tone) = match BUSY.lock().unwrap().count {
        None => ("Checking…".to_string(), Tone::None),
        Some(0) => ("Ready to eject".to_string(), Tone::Ok),
        Some(1) => ("1 app using the SSD".to_string(), Tone::Warn),
        Some(n) => (format!("{n} apps using the SSD"), Tone::Warn),
    };
    let text = match snap.workspace_space {
        Some(sp) => format!("{state} · {} free", system::gb(sp.free)),
        None => state,
    };
    (text, tone)
}

/// 26×26 badge: a filled circle with the drive symbol, plus a status dot. Drawn on demand,
/// so the system colors follow the menu's light/dark appearance.
fn header_image(mounted: bool, tone: Tone) -> Option<Retained<NSImage>> {
    let sym = NSImage::imageWithSystemSymbolName_accessibilityDescription(
        &NSString::from_str("externaldrive.fill"),
        None,
    )?;
    let color = if mounted {
        NSColor::labelColor()
    } else {
        NSColor::tertiaryLabelColor()
    };
    let config = NSImageSymbolConfiguration::configurationWithHierarchicalColor(&color);
    let sym = sym.imageWithSymbolConfiguration(&config)?;
    let size = sym.size();
    let draw = RcBlock::new(move |rect: NSRect| {
        NSColor::tertiarySystemFillColor().setFill();
        NSBezierPath::bezierPathWithOvalInRect(rect).fill();
        let origin = NSPoint::new(
            ((rect.size.width - size.width) / 2.0).round(),
            ((rect.size.height - size.height) / 2.0).round(),
        );
        sym.drawInRect(NSRect::new(origin, size));
        let dot = match tone {
            Tone::None => None,
            Tone::Ok => Some(NSColor::systemGreenColor()),
            Tone::Warn => Some(NSColor::systemOrangeColor()),
        };
        if let Some(dot) = dot {
            dot.setFill();
            let d = 8.0;
            NSBezierPath::bezierPathWithOvalInRect(NSRect::new(
                NSPoint::new(rect.size.width - d, 0.0),
                NSSize::new(d, d),
            ))
            .fill();
        }
        Bool::YES
    });
    Some(NSImage::imageWithSize_flipped_drawingHandler(
        NSSize::new(26.0, 26.0),
        false,
        &draw,
    ))
}

fn apply_header(item: &NSMenuItem) {
    let (subtitle, tone) = header_content();
    let mounted = SNAPSHOT
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.workspace_mounted);
    item.setSubtitle(Some(&NSString::from_str(&subtitle)));
    item.setImage(header_image(mounted, tone).as_deref());
}

// ---------------------------------------------------------------------------------------
// Styling the menu as it opens

fn style(menu: &NSMenu) {
    let Some(mtm) = MainThreadMarker::new() else {
        return;
    };
    let layout = LAYOUT.lock().unwrap().clone();
    for (index, role) in layout {
        let index = index as isize;
        if index >= menu.numberOfItems() {
            continue;
        }
        let Some(item) = menu.itemAtIndex(index) else {
            continue;
        };
        match role {
            Role::Header => {
                let font = NSFont::boldSystemFontOfSize(13.0);
                // SAFETY: an NSFont is a valid value for NSFontAttributeName.
                let attrs = NSDictionary::<NSString, AnyObject>::from_slices(
                    &[unsafe { NSFontAttributeName }],
                    &[font.as_ref()],
                );
                let title =
                    unsafe { NSAttributedString::new_with_attributes(&item.title(), &attrs) };
                item.setAttributedTitle(Some(&title));
                apply_header(&item);
                HEADER.with(|h| *h.borrow_mut() = Some(item));
            }
            Role::Section => {
                if !item.isSectionHeader() {
                    let header = NSMenuItem::sectionHeaderWithTitle(&item.title(), mtm);
                    menu.removeItemAtIndex(index);
                    menu.insertItem_atIndex(&header, index);
                }
            }
            // 12 pt per level: 2 lines the text up with the titles of items with an icon.
            Role::Indent => item.setIndentationLevel(2),
        }
    }
    check_busy();
}

/// Refresh eject readiness in the background, then update the header if it is still shown.
fn check_busy() {
    let mounted = SNAPSHOT
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|s| s.workspace_mounted && !s.busy);
    if !mounted {
        return;
    }
    {
        let mut b = BUSY.lock().unwrap();
        if b.running || b.finished.is_some_and(|t| t.elapsed() < BUSY_MIN_INTERVAL) {
            return;
        }
        b.running = true;
    }
    std::thread::spawn(|| {
        let count = APP
            .get()
            .map_or(0, |app| crate::list_holders_for(app).len());
        {
            let mut b = BUSY.lock().unwrap();
            b.count = Some(count);
            b.finished = Some(Instant::now());
            b.running = false;
        }
        if let Some(app) = APP.get() {
            let _ = app.run_on_main_thread(|| {
                HEADER.with(|h| {
                    if let Some(item) = h.borrow().as_ref() {
                        apply_header(item);
                    }
                })
            });
        }
    });
}

// ---------------------------------------------------------------------------------------
// -[NSStatusItem setMenu:] hook

type SetMenu = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

static ORIGINAL_SET_MENU: OnceLock<SetMenu> = OnceLock::new();

unsafe extern "C-unwind" fn set_menu_styled(this: *mut AnyObject, cmd: Sel, menu: *mut AnyObject) {
    // SAFETY: `menu` is the NSMenu argument of -setMenu: (or nil).
    if let Some(menu) = unsafe { (menu as *const NSMenu).as_ref() } {
        style(menu);
    }
    if let Some(original) = ORIGINAL_SET_MENU.get() {
        // SAFETY: the original implementation, called with its own receiver and arguments.
        unsafe { original(this, cmd, menu) };
    }
}

fn hook_status_item_set_menu() {
    if ORIGINAL_SET_MENU.get().is_some() {
        return;
    }
    let Some(method) =
        AnyClass::get(c"NSStatusItem").and_then(|c| c.instance_method(sel!(setMenu:)))
    else {
        return;
    };
    // SAFETY: `set_menu_styled` has the exact `-(void)setMenu:(NSMenu *)` signature, and the
    // original implementation is stored before ours is installed.
    unsafe {
        let original = std::mem::transmute::<Imp, SetMenu>(method.implementation());
        let _ = ORIGINAL_SET_MENU.set(original);
        method.set_implementation(std::mem::transmute::<SetMenu, Imp>(set_menu_styled));
    }
}
