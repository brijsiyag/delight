//! Plugin files dropped on the menu bar icon.
//!
//! The icon's button has a view of `tray-icon`'s over it, which takes the clicks, and neither
//! knows about drags. So that view is given the three methods of a drop target (its class is
//! replaced by a subclass with them) and asked for file drags. Nothing else about it changes: clicks
//! and the menu work as before. The files go, as paths, through a channel to the app, which runs
//! them through the same install flow as a file picked in Settings.
//!
//! AppKit calls these on the main thread, in the middle of its own event handling, so they only
//! read the pasteboard and send; GPUI is not touched from here.

use std::path::{Path, PathBuf};
use std::sync::OnceLock;

use futures::channel::mpsc::UnboundedSender;
use objc2::rc::Retained;
use objc2::runtime::{AnyClass, AnyObject, Bool, ClassBuilder, Sel};
use objc2::{MainThreadMarker, msg_send, sel};
use objc2_app_kit::NSStatusItem;
use objc2_foundation::{NSArray, NSString};

/// The pasteboard type Finder puts a drag's files under: an array of paths.
const FILENAMES: &str = "NSFilenamesPboardType";
/// `NSDragOperationCopy`, and `NSDragOperationNone`.
const COPY: usize = 1;
const NONE: usize = 0;

static DROPS: OnceLock<UnboundedSender<Vec<PathBuf>>> = OnceLock::new();

/// Send the `.wasm` files dropped on the icon to `drops`.
pub fn accept_drops(item: &NSStatusItem, drops: UnboundedSender<Vec<PathBuf>>) {
    let Some(mtm) = MainThreadMarker::new() else { return };
    let Some(button) = item.button(mtm) else { return };
    // The last view added over the button is tray-icon's.
    let Some(target) = button.subviews().lastObject() else {
        log::warn!("the menu bar icon has no view to take drops");
        return;
    };
    if DROPS.set(drops).is_err() {
        return;
    }
    let target: &AnyObject = &target;
    let Some(mut builder) = ClassBuilder::new(c"DelightTrayDropTarget", target.class()) else {
        log::warn!("making the menu bar icon's drop target: its class exists already");
        return;
    };
    // SAFETY: the signatures are NSDraggingDestination's: `(id<NSDraggingInfo>) -> NSDragOperation`
    // and `(id<NSDraggingInfo>) -> BOOL`; the class adds no instance variables.
    unsafe {
        builder.add_method(sel!(draggingEntered:), dragging_entered as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize);
        builder.add_method(sel!(draggingUpdated:), dragging_entered as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> usize);
        builder.add_method(sel!(prepareForDragOperation:), prepare as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool);
        builder.add_method(sel!(performDragOperation:), perform as unsafe extern "C-unwind" fn(*mut AnyObject, Sel, *mut AnyObject) -> Bool);
    }
    let class: &'static AnyClass = builder.register();
    // SAFETY: the subclass has the layout of its superclass, which this object is an instance of.
    unsafe { objc2::ffi::object_setClass((target as *const AnyObject).cast_mut(), class) };
    let types = NSArray::from_retained_slice(&[NSString::from_str(FILENAMES)]);
    // SAFETY: `registerForDraggedTypes:` takes an array of pasteboard type names.
    unsafe { msg_send![target, registerForDraggedTypes: &*types] }
}

/// The plugin files (`.wasm`) among the paths a drag carries.
fn wasm_files(sender: &AnyObject) -> Vec<PathBuf> {
    // SAFETY: `draggingPasteboard` is NSDraggingInfo's, `propertyListForType:` NSPasteboard's; for
    // Finder's file drags it is an array of path strings.
    let paths: Option<Retained<NSArray<NSString>>> = unsafe {
        let pasteboard: Option<Retained<AnyObject>> = msg_send![sender, draggingPasteboard];
        let Some(pasteboard) = pasteboard else { return Vec::new() };
        msg_send![&*pasteboard, propertyListForType: &*NSString::from_str(FILENAMES)]
    };
    let paths = paths.map(|paths| paths.iter().map(|path| PathBuf::from(path.to_string())).collect::<Vec<_>>());
    only_wasm(paths.unwrap_or_default())
}

fn only_wasm(paths: Vec<PathBuf>) -> Vec<PathBuf> {
    paths.into_iter().filter(|path| is_wasm(path)).collect()
}

fn is_wasm(path: &Path) -> bool {
    path.extension().is_some_and(|extension| extension.eq_ignore_ascii_case("wasm"))
}

/// The dragging info AppKit passes, as a reference for the length of the call.
///
/// # Safety
/// `sender` is a live `id<NSDraggingInfo>` for the duration of the call, which AppKit guarantees.
unsafe fn info<'a>(sender: *mut AnyObject) -> Option<&'a AnyObject> {
    unsafe { sender.as_ref() }
}

/// A drag over the icon: it copies if it carries a plugin.
unsafe extern "C-unwind" fn dragging_entered(_: *mut AnyObject, _: Sel, sender: *mut AnyObject) -> usize {
    match unsafe { info(sender) } {
        Some(sender) if !wasm_files(sender).is_empty() => COPY,
        _ => NONE,
    }
}

unsafe extern "C-unwind" fn prepare(_: *mut AnyObject, _: Sel, sender: *mut AnyObject) -> Bool {
    Bool::new(unsafe { info(sender) }.is_some_and(|sender| !wasm_files(sender).is_empty()))
}

/// The drop: the plugins go to the app.
unsafe extern "C-unwind" fn perform(_: *mut AnyObject, _: Sel, sender: *mut AnyObject) -> Bool {
    let files = unsafe { info(sender) }.map(wasm_files).unwrap_or_default();
    if files.is_empty() {
        return Bool::NO;
    }
    Bool::new(DROPS.get().is_some_and(|drops| drops.unbounded_send(files).is_ok()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_wasm_files_are_taken() {
        let paths = ["/a/x.wasm", "/a/Y.WASM", "/a/readme.md", "/a/noext", "/a/plugin.wasm.bak"].map(PathBuf::from).to_vec();
        assert_eq!(only_wasm(paths), [PathBuf::from("/a/x.wasm"), PathBuf::from("/a/Y.WASM")]);
    }
}
