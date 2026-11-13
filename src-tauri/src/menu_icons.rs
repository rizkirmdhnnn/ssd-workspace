//! SF Symbols next to the tray menu items.
//!
//! Two obstacles, both worked around here:
//!
//! 1. **macOS 27 hides menu item images by default.** An item only shows its image when its
//!    `preferredImageVisibility` is `NSMenuItemImageVisibilityVisible` (new in macOS 27).
//!    Tauri/muda never set it and do not expose their `NSMenuItem`s, so
//!    `-[NSMenuItem setImage:]` is wrapped (in this process only) to also mark any item
//!    that receives an image as visible.
//! 2. **Tauri only takes a full-color PNG (black in dark mode) or a fixed list of system
//!    image names (`NativeIcon`).** So each symbol is drawn centered on an 18×18 template
//!    canvas (muda forces menu images to 18×18, which would otherwise stretch non-square
//!    symbols) and registered under the name of an obsolete system image this app never
//!    shows (MobileMe, iChat Theater, …). `imageNamed:` looks in the app's own registry
//!    first, so those `NativeIcon`s now show our symbols.

use block2::RcBlock;
use objc2::runtime::{AnyClass, AnyObject, Bool, Imp, Sel};
use objc2::{msg_send, sel};
use objc2_app_kit::{
    NSImage, NSImageName, NSImageNameColumnViewTemplate, NSImageNameFlowViewTemplate,
    NSImageNameIChatTheaterTemplate, NSImageNameIconViewTemplate, NSImageNameMobileMe,
    NSImageNameQuickLookTemplate, NSImageNameSlideshowTemplate,
};
use objc2_foundation::{NSPoint, NSRect, NSSize, NSString};
use std::sync::OnceLock;
use tauri::menu::NativeIcon;

/// A menu image: the `NativeIcon` slot it occupies, and the SF Symbol drawn into it.
#[derive(Clone, Copy)]
pub enum Icon {
    Drive,
    Eject,
    Folder,
    BackUp,
    Skip,
    Details,
    Options,
}

const ALL: [Icon; 7] = [
    Icon::Drive,
    Icon::Eject,
    Icon::Folder,
    Icon::BackUp,
    Icon::Skip,
    Icon::Details,
    Icon::Options,
];

impl Icon {
    fn symbol(self) -> &'static str {
        match self {
            Icon::Drive => "externaldrive",
            Icon::Eject => "eject",
            Icon::Folder => "folder",
            Icon::BackUp => "clock.arrow.circlepath",
            Icon::Skip => "xmark.circle",
            Icon::Details => "macwindow",
            Icon::Options => "gearshape",
        }
    }

    fn slot(self) -> NativeIcon {
        match self {
            Icon::Drive => NativeIcon::MobileMe,
            Icon::Eject => NativeIcon::IChatTheater,
            Icon::Folder => NativeIcon::QuickLook,
            Icon::BackUp => NativeIcon::Slideshow,
            Icon::Skip => NativeIcon::FlowView,
            Icon::Details => NativeIcon::ColumnView,
            Icon::Options => NativeIcon::IconView,
        }
    }

    /// The system image name behind `slot()` (what muda passes to `imageNamed:`).
    fn slot_name(self) -> &'static NSImageName {
        // SAFETY: AppKit constants, valid for the life of the process.
        unsafe {
            match self {
                Icon::Drive => NSImageNameMobileMe,
                Icon::Eject => NSImageNameIChatTheaterTemplate,
                Icon::Folder => NSImageNameQuickLookTemplate,
                Icon::BackUp => NSImageNameSlideshowTemplate,
                Icon::Skip => NSImageNameFlowViewTemplate,
                Icon::Details => NSImageNameColumnViewTemplate,
                Icon::Options => NSImageNameIconViewTemplate,
            }
        }
    }
}

pub fn native(icon: Icon) -> Option<NativeIcon> {
    Some(icon.slot())
}

const CANVAS: f64 = 18.0;

/// Register every symbol and make menu images visible on macOS 27. Call once on the main
/// thread before the first menu is built. A symbol missing on this macOS version simply
/// leaves its item without an image.
pub fn register() {
    show_menu_images();
    for icon in ALL {
        let Some(sym) = NSImage::imageWithSystemSymbolName_accessibilityDescription(
            &NSString::from_str(icon.symbol()),
            None,
        ) else {
            continue;
        };
        let size = sym.size();
        let draw = RcBlock::new(move |rect: NSRect| {
            let origin = NSPoint::new(
                ((rect.size.width - size.width) / 2.0).round(),
                ((rect.size.height - size.height) / 2.0).round(),
            );
            sym.drawInRect(NSRect::new(origin, size));
            Bool::YES
        });
        let img = NSImage::imageWithSize_flipped_drawingHandler(
            NSSize::new(CANVAS, CANVAS),
            false,
            &draw,
        );
        img.setTemplate(true);
        if !img.setName(Some(icon.slot_name())) {
            eprintln!(
                "[ssd-workspace] menu icon {:?} not registered",
                icon.symbol()
            );
        }
        // The named-image registry must keep it for the life of the app.
        std::mem::forget(img);
    }
}

// ---------------------------------------------------------------------------------------
// macOS 27: -[NSMenuItem setImage:] also sets preferredImageVisibility = Visible

/// `NSMenuItemImageVisibilityVisible` (AppKit/NSMenuItem.h, macOS 27).
const IMAGE_VISIBILITY_VISIBLE: isize = 1;

type SetImage = unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject);

static ORIGINAL_SET_IMAGE: OnceLock<SetImage> = OnceLock::new();

unsafe extern "C-unwind" fn set_image_visible(
    this: *mut AnyObject,
    cmd: Sel,
    image: *mut AnyObject,
) {
    if let Some(original) = ORIGINAL_SET_IMAGE.get() {
        // SAFETY: the original implementation, called with its own receiver and arguments.
        unsafe { original(this, cmd, image) };
    }
    if !image.is_null() {
        // SAFETY: only installed when NSMenuItem responds to this selector (macOS 27+).
        let _: () =
            unsafe { msg_send![this, setPreferredImageVisibility: IMAGE_VISIBILITY_VISIBLE] };
    }
}

fn show_menu_images() {
    let Some(class) = AnyClass::get(c"NSMenuItem") else {
        return;
    };
    // Older macOS shows menu images anyway.
    if class
        .instance_method(sel!(setPreferredImageVisibility:))
        .is_none()
    {
        return;
    }
    let Some(method) = class.instance_method(sel!(setImage:)) else {
        return;
    };
    if ORIGINAL_SET_IMAGE.get().is_some() {
        return;
    }
    // SAFETY: `set_image_visible` has the exact `-(void)setImage:(NSImage *)` signature, and
    // the previous implementation is stored before any menu item is created.
    unsafe {
        let original = std::mem::transmute::<Imp, SetImage>(method.implementation());
        let _ = ORIGINAL_SET_IMAGE.set(original);
        method.set_implementation(std::mem::transmute::<SetImage, Imp>(set_image_visible));
    }
}
