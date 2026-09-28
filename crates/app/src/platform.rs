//! AppKit calls GPUI doesn't expose: the menu-bar-only app, the launcher's panel
//! look (a rounded native backdrop), resizing that keeps the top edge in place,
//! and showing and hiding without closing.
//!
//! AppKit calls back into GPUI synchronously when a window changes (it resizes, it
//! becomes or stops being key). GPUI drops those callbacks while one of its updates
//! is running ("RefCell already borrowed"), and every `update`, `defer` and window
//! callback is one. So what changes a window is a [`NativeWindow`] method, called
//! from a spawned task, outside any update.

use gpui::Window;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, Message, msg_send};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSApplication, NSApplicationActivationOptions, NSApplicationActivationPolicy,
    NSAutoresizingMaskOptions, NSBezierPath, NSColor, NSGlassEffectView, NSImage, NSImageResizingMode,
    NSRunningApplication, NSView, NSVisualEffectBlendingMode, NSVisualEffectMaterial, NSVisualEffectState,
    NSVisualEffectView, NSWindow, NSWindowOrderingMode, NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};
use raw_window_handle::{HasWindowHandle, RawWindowHandle};

fn main_thread() -> Option<MainThreadMarker> {
    MainThreadMarker::new()
}

/// GPUI's view: it draws the window and takes the keyboard.
fn gpui_view(window: &Window) -> Option<Retained<NSView>> {
    let RawWindowHandle::AppKit(handle) = HasWindowHandle::window_handle(window).ok()?.as_raw() else {
        return None;
    };
    // SAFETY: GPUI's AppKit handle points at the window's live NSView.
    let view: &NSView = unsafe { handle.ns_view.cast::<NSView>().as_ref() };
    Some(view.retain())
}

/// No Dock icon and no app menu: Delight lives in the menu bar. (The bundled app
/// also says so in its Info.plist, `LSUIElement`; this covers running it unbundled.)
pub fn set_accessory_app() {
    let Some(mtm) = main_thread() else { return };
    NSApplication::sharedApplication(mtm).setActivationPolicy(NSApplicationActivationPolicy::Accessory);
}

/// Whether the window is on screen.
pub fn is_window_visible(window: &Window) -> bool {
    gpui_view(window).and_then(|view| view.window()).is_some_and(|win| win.isVisible())
}

/// The material behind the launcher's content.
enum Backdrop {
    /// Liquid Glass (macOS 26+), as Spotlight uses: its own translucent fill, light
    /// rim and rounded shape.
    Glass(Retained<NSGlassEffectView>),
    /// Older macOS: a blur, shaped by a rounded mask.
    Blur(Retained<NSVisualEffectView>),
}

thread_local! {
    /// The launcher's backdrop (there's one launcher window).
    static BACKDROP: std::cell::RefCell<Option<Backdrop>> = const { std::cell::RefCell::new(None) };

    /// The app that was in front when the launcher was shown, to go back to when
    /// it hides.
    static PREVIOUS_APP: std::cell::RefCell<Option<Retained<NSRunningApplication>>> =
        const { std::cell::RefCell::new(None) };
}

fn new_backdrop(mtm: MainThreadMarker, frame: NSRect) -> (Backdrop, Retained<NSView>) {
    // NSGlassEffectView exists only on macOS 26+.
    if AnyClass::get(c"NSGlassEffectView").is_some() {
        let glass = NSGlassEffectView::initWithFrame(NSGlassEffectView::alloc(mtm), frame);
        let view = Retained::clone(&glass).into_super();
        return (Backdrop::Glass(glass), view);
    }
    let blur = NSVisualEffectView::initWithFrame(NSVisualEffectView::alloc(mtm), frame);
    blur.setMaterial(NSVisualEffectMaterial::Popover);
    blur.setBlendingMode(NSVisualEffectBlendingMode::BehindWindow);
    blur.setState(NSVisualEffectState::Active);
    let view = Retained::clone(&blur).into_super();
    (Backdrop::Blur(blur), view)
}

/// Whether the launcher sits on Liquid Glass (it then draws a light rim).
pub fn uses_liquid_glass() -> bool {
    BACKDROP.with_borrow(|backdrop| matches!(backdrop, Some(Backdrop::Glass(_))))
}

/// A stretchable rounded-rect mask image (its corners stay fixed).
fn rounded_mask(radius: f64) -> Retained<NSImage> {
    let side = radius * 2. + 1.;
    let size = NSSize::new(side, side);
    let image = NSImage::initWithSize(NSImage::alloc(), size);
    #[allow(deprecated)] // lockFocus: the simplest way to draw into an image
    {
        image.lockFocus();
        NSColor::blackColor().set();
        let rect = NSRect::new(NSPoint::new(0., 0.), size);
        NSBezierPath::bezierPathWithRoundedRect_xRadius_yRadius(rect, radius, radius).fill();
        image.unlockFocus();
    }
    image.setCapInsets(NSEdgeInsets { top: radius, left: radius, bottom: radius, right: radius });
    image.setResizingMode(NSImageResizingMode::Stretch);
    image
}

/// A GPUI window's AppKit side. Take it inside an update ([`NativeWindow::of`]), and
/// call its methods outside one (see the module's notes).
#[derive(Clone)]
pub struct NativeWindow {
    window: Retained<NSWindow>,
    /// GPUI's view: it draws the window and takes the keyboard.
    view: Retained<NSView>,
}

impl NativeWindow {
    pub fn of(window: &Window) -> Option<Self> {
        let view = gpui_view(window)?;
        Some(Self { window: view.window()?, view })
    }

    /// The Spotlight panel look: a native backdrop behind GPUI's view, with a system
    /// shadow that follows its shape. (GPUI's own blurred background keeps macOS's
    /// corner radius, not ours.)
    pub fn style_floating_panel(&self, radius: f64) {
        let Some(mtm) = main_thread() else { return };
        let win = &self.window;
        // Borderless: GPUI makes a titled window, and macOS draws a titled window's
        // own rounded frame and edge line around our shape. (GPUI's windows can become
        // key without a title bar.) Not a non-activating panel: `present` activates
        // Delight.
        win.setStyleMask(NSWindowStyleMask::Borderless);
        // Changing the style rebuilds the window's frame and hands the keyboard to
        // the window itself: give it back to GPUI's view, or every key press just
        // beeps.
        win.makeFirstResponder(Some(&self.view));
        let content = win.contentView();
        let has_backdrop = BACKDROP.with_borrow(Option::is_some);
        if let (Some(content), false) = (&content, has_backdrop)
            // SAFETY: the content view's superview is the window's frame view, alive
            // while the window is.
            && let Some(frame_view) = unsafe { content.superview() }
        {
            let (backdrop, view) = new_backdrop(mtm, frame_view.bounds());
            view.setAutoresizingMask(
                NSAutoresizingMaskOptions::ViewWidthSizable | NSAutoresizingMaskOptions::ViewHeightSizable,
            );
            frame_view.addSubview_positioned_relativeTo(&view, NSWindowOrderingMode::Below, Some(content));
            BACKDROP.set(Some(backdrop));
        }
        // Delight decides when to hide (Esc, losing focus, the hotkey).
        win.setHidesOnDeactivate(false);
        win.setHasShadow(true);
        self.set_corner_radius(radius);
    }

    /// Round the backdrop and GPUI's content (continuous "squircle" corners).
    pub fn set_corner_radius(&self, radius: f64) {
        BACKDROP.with_borrow(|backdrop| match backdrop {
            Some(Backdrop::Glass(glass)) => glass.setCornerRadius(radius),
            Some(Backdrop::Blur(blur)) => blur.setMaskImage(Some(&rounded_mask(radius))),
            None => {}
        });
        // Clip GPUI's content, and the glass, whose effect otherwise reaches into the
        // window's square corners, to the same rounded shape.
        let glass = BACKDROP.with_borrow(|backdrop| match backdrop {
            Some(Backdrop::Glass(glass)) => Some(Retained::clone(glass).into_super()),
            _ => None,
        });
        let continuous = NSString::from_str("continuous");
        for view in self.window.contentView().into_iter().chain(glass) {
            view.setWantsLayer(true);
            let Some(layer) = view.layer() else { continue };
            // SAFETY: plain CALayer property setters.
            unsafe {
                let _: () = msg_send![&*layer, setCornerRadius: radius];
                let _: () = msg_send![&*layer, setMasksToBounds: true];
                let _: () = msg_send![&*layer, setCornerCurve: &*continuous];
            }
        }
        self.window.invalidateShadow();
    }

    /// Resize keeping the top edge fixed and the window centred (AppKit anchors the
    /// bottom-left), so the panel grows down from the input bar.
    pub fn resize_keep_top(&self, width: f64, height: f64, animate: bool) {
        let win = &self.window;
        let frame = win.frame();
        let chrome = frame.size.height - win.contentRectForFrameRect(frame).size.height;
        let new_height = height + chrome;
        if (new_height - frame.size.height).abs() < 0.5 && (width - frame.size.width).abs() < 0.5 {
            return;
        }
        let top = frame.origin.y + frame.size.height;
        let x = frame.origin.x + (frame.size.width - width) / 2.;
        let new_frame = NSRect::new(NSPoint::new(x, top - new_height), NSSize::new(width, new_height));
        if animate {
            // The animator proxy animates asynchronously (no nested run loop).
            win.animator().setFrame_display(new_frame, true);
        } else {
            win.setFrame_display(new_frame, true);
        }
        win.invalidateShadow();
    }

    /// Bring the window forward as the key window and activate Delight, as Raycast
    /// and Alfred do: every key goes to the launcher. (A non-activating panel, as
    /// Spotlight is, leaves the previous app active, and macOS sends it modifier-only
    /// keys.) Remembers the app that was in front, for [`Self::hide`].
    pub fn present(&self) {
        let Some(mtm) = main_thread() else { return };
        let front = NSWorkspace::sharedWorkspace().frontmostApplication();
        let current = NSRunningApplication::currentApplication().processIdentifier();
        if let Some(front) = front.filter(|app| app.processIdentifier() != current) {
            PREVIOUS_APP.set(Some(front));
        }
        NSApplication::sharedApplication(mtm).activate();
        self.window.orderFrontRegardless();
        self.window.makeKeyWindow();
    }

    /// Hide without closing, so all state survives. If the launcher had the keyboard
    /// (hidden by Esc or the hotkey), the app that was in front before comes back;
    /// not if another app was clicked.
    pub fn hide(&self) {
        let Some(mtm) = main_thread() else { return };
        let had_keyboard = self.window.isKeyWindow() && NSApplication::sharedApplication(mtm).isActive();
        self.window.orderOut(None);
        if let Some(previous) = PREVIOUS_APP.take()
            && had_keyboard
        {
            previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
    }
}
