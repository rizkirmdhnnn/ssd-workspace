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
//! - **Progress**: the Time Machine status line, replaced by a view with the status, a
//!   progress bar and the time remaining (like the built-in Time Machine menu). It is
//!   updated in place on every status refresh, also while the menu is open.
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
    NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSControlSize, NSFont, NSFontAttributeName,
    NSImage, NSImageSymbolConfiguration, NSLineBreakMode, NSMenu, NSMenuItem, NSProgressIndicator,
    NSProgressIndicatorStyle, NSTextField, NSView,
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
    Progress,
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
    /// The backup progress view of the installed menu; main thread only.
    static PROGRESS: RefCell<Option<ProgressView>> = const { RefCell::new(None) };
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
    let backing_up = s.tm.running;
    *cur = Some(s.clone());
    drop(cur);
    if backing_up && let Some(app) = APP.get() {
        let _ = app.run_on_main_thread(|| {
            PROGRESS.with(|p| {
                if let Some(view) = p.borrow().as_ref() {
                    view.update();
                }
            })
        });
    }
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
    let apps = |n: usize| format!("{n} app{} using the SSD", if n == 1 { "" } else { "s" });
    let free = snap
        .workspace_space
        .map(|sp| format!("{} free", system::gb(sp.free)));
    let count = BUSY.lock().unwrap().count;
    // A backup to the SSD is not finished yet: never "Ready to eject" while it runs.
    let (parts, tone) = if snap.backing_up_to_ssd() {
        match count {
            Some(n) if n > 0 => (vec![Some("Backing up".into()), Some(apps(n))], Tone::Warn),
            _ => (vec![Some("Backing up".into()), free], Tone::Warn),
        }
    } else {
        match count {
            None => (vec![Some("Checking…".into()), free], Tone::None),
            Some(0) => (vec![Some("Ready to eject".into()), free], Tone::Ok),
            Some(n) => (vec![Some(apps(n)), free], Tone::Warn),
        }
    };
    (
        parts.into_iter().flatten().collect::<Vec<_>>().join(" · "),
        tone,
    )
}

// ---------------------------------------------------------------------------------------
// Backup progress

/// "Backing up: 1.2 GB of 4.2 GB", a progress bar, and "About 3 minutes remaining".
struct ProgressView {
    status: Retained<NSTextField>,
    bar: Retained<NSProgressIndicator>,
    remaining: Retained<NSTextField>,
}

/// Leading inset that lines the text up with the titles of items without an icon.
const PROGRESS_INSET: f64 = 14.5;
const PROGRESS_WIDTH: f64 = 250.0;
const PROGRESS_HEIGHT: f64 = 54.0;

impl ProgressView {
    fn new(mtm: MainThreadMarker) -> (Retained<NSView>, Self) {
        let frame =
            |x: f64, y: f64, w: f64, h: f64| NSRect::new(NSPoint::new(x, y), NSSize::new(w, h));
        let inner = PROGRESS_WIDTH - 2.0 * PROGRESS_INSET;
        let view = NSView::initWithFrame(
            mtm.alloc(),
            frame(0.0, 0.0, PROGRESS_WIDTH, PROGRESS_HEIGHT),
        );
        // NSMenu stretches a view to the menu's width only when it is width-sizable.
        view.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        let label = |size: f64, color: &NSColor, y: f64, h: f64| {
            let field = NSTextField::labelWithString(&NSString::new(), mtm);
            field.setFont(Some(&NSFont::menuFontOfSize(size)));
            field.setTextColor(Some(color));
            field.setLineBreakMode(NSLineBreakMode::ByTruncatingTail);
            field.setFrame(frame(PROGRESS_INSET, y, inner, h));
            field.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
            view.addSubview(&field);
            field
        };
        // Bottom to top (the view is not flipped).
        let remaining = label(11.0, &NSColor::tertiaryLabelColor(), 5.0, 14.0);
        let bar = NSProgressIndicator::initWithFrame(
            mtm.alloc(),
            // A label draws its text 2 pt in from its frame; the bar has no such padding.
            frame(PROGRESS_INSET + 2.0, 20.0, inner - 4.0, 12.0),
        );
        bar.setStyle(NSProgressIndicatorStyle::Bar);
        bar.setControlSize(NSControlSize::Small);
        bar.setMinValue(0.0);
        bar.setMaxValue(1.0);
        bar.setAutoresizingMask(NSAutoresizingMaskOptions::ViewWidthSizable);
        view.addSubview(&bar);
        let status = label(13.0, &NSColor::secondaryLabelColor(), 33.0, 17.0);
        let this = ProgressView {
            status,
            bar,
            remaining,
        };
        this.update();
        (view, this)
    }

    /// Show the latest status. Before copying starts and while finishing, Time Machine has
    /// no fraction: the bar is indeterminate then.
    fn update(&self) {
        let snap = SNAPSHOT.lock().unwrap().clone().unwrap_or_default();
        let tm = &snap.tm;
        self.status
            .setStringValue(&NSString::from_str(&tm.status_text));
        let remaining = tm
            .remaining_text
            .as_deref()
            .unwrap_or(if snap.backing_up_to_ssd() {
                "Ejecting the SSD stops the backup"
            } else {
                ""
            });
        self.remaining
            .setStringValue(&NSString::from_str(remaining));
        let fraction = tm
            .progress
            .as_ref()
            .filter(|p| p.phase == crate::timemachine::Phase::Copying)
            .and_then(|p| p.fraction);
        match fraction {
            Some(f) => {
                if self.bar.isIndeterminate() {
                    // SAFETY: plain AppKit calls on the main thread.
                    unsafe { self.bar.stopAnimation(None) };
                    self.bar.setIndeterminate(false);
                }
                self.bar.setDoubleValue(f);
            }
            None => {
                self.bar.setIndeterminate(true);
                // SAFETY: plain AppKit call on the main thread.
                unsafe { self.bar.startAnimation(None) };
            }
        }
    }
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
            Role::Progress => {
                if item.view().is_none() {
                    let (view, progress) = ProgressView::new(mtm);
                    item.setView(Some(&view));
                    PROGRESS.with(|p| *p.borrow_mut() = Some(progress));
                } else {
                    PROGRESS.with(|p| p.borrow().as_ref().map(ProgressView::update));
                }
            }
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
