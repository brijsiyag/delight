//! AppKit calls GPUI doesn't expose: the menu-bar-only app, the launcher's panel
//! look (a rounded native backdrop), resizing that keeps the top edge in place,
//! and showing and hiding without closing. Each is a workaround, listed in the
//! README with what GPUI would need to offer before it can go.
//!
//! AppKit calls back into GPUI synchronously when a window changes (it resizes, it
//! becomes or stops being key). GPUI drops those callbacks while one of its updates
//! is running ("RefCell already borrowed"), and every `update`, `defer` and window
//! callback is one. So what changes a window is a [`NativeWindow`] method, called
//! from a spawned task, outside any update.

use std::cell::Cell;
use std::time::Duration;

use gpui::Window;
use objc2::rc::Retained;
use objc2::runtime::AnyClass;
use objc2::{AnyThread, MainThreadMarker, MainThreadOnly, Message, msg_send};
use objc2_app_kit::{
    NSAnimatablePropertyContainer, NSAnimationContext, NSAppearance, NSAppearanceNameAqua, NSAppearanceNameDarkAqua, NSApplication,
    NSApplicationActivationOptions, NSApplicationActivationPolicy, NSAutoresizingMaskOptions, NSBezierPath, NSColor,
    NSGlassEffectView, NSImage, NSImageResizingMode, NSRunningApplication, NSView, NSVisualEffectBlendingMode,
    NSVisualEffectMaterial, NSVisualEffectState, NSVisualEffectView, NSWindow, NSWindowOrderingMode,
    NSWindowStyleMask, NSWorkspace,
};
use objc2_foundation::{NSEdgeInsets, NSPoint, NSRect, NSSize, NSString};
use objc2_quartz_core::{
    CAMediaTimingFunction, CAMediaTimingFunctionName, kCAMediaTimingFunctionEaseInEaseOut, kCAMediaTimingFunctionEaseOut,
};
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

/// Draw Delight's windows light (`Some(false)`), dark (`Some(true)`), or as macOS does
/// (`None`), so their native parts (the blur, the glass, the title bar) match the
/// Appearance setting. macOS redraws the windows and tells GPUI: call it outside a
/// GPUI update.
pub fn set_app_appearance(dark: Option<bool>) {
    let Some(mtm) = main_thread() else { return };
    let appearance = dark.and_then(|dark| {
        // SAFETY: AppKit's appearance name constants, only read.
        let name = unsafe { if dark { NSAppearanceNameDarkAqua } else { NSAppearanceNameAqua } };
        NSAppearance::appearanceNamed(name)
    });
    NSApplication::sharedApplication(mtm).setAppearance(appearance.as_deref());
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

    /// Each coming of the launcher, and each resize: [`NativeWindow::settle`] eases out only the
    /// latest coming, and not once the window was resized since.
    static COMING: Cell<u64> = const { Cell::new(0) };
    /// Each going of the launcher: [`NativeWindow::hide`] takes it off screen only if it hasn't
    /// come again, or started going again, since.
    static GOING: Cell<u64> = const { Cell::new(0) };
    /// The launcher is fading away (between [`NativeWindow::leave`] and [`NativeWindow::hide`]).
    static LEAVING: Cell<bool> = const { Cell::new(false) };
    /// The frame the launcher rests at while its coming or going moves it; `None` while nothing of
    /// ours moves it.
    static REST: Cell<Option<NSRect>> = const { Cell::new(None) };
}

/// The launcher comes and goes as Spotlight does on macOS 26, measured frame by frame from a 57 fps
/// recording, with a gentler swing: it fades in over 140 ms while it narrows, from 4% wider than it
/// rests to 0.5% narrower at 158 ms, then eases back out to its width by about 300 ms; its height
/// takes no part. (Spotlight swings from 10% wider to 1.3% narrower: too much, on 2026-10-01.)
/// It goes in 120 ms, fading while it grows a little all round. (Spotlight also blurs as it goes:
/// only a private filter does that.) With Reduce Motion on, it only fades.
const ENTER_FADE: f64 = 0.14;
const ENTER_WIDER: f64 = 0.04;
const ENTER_NARROWER: f64 = 0.005;
/// From the start of a coming to its narrowest, when [`NativeWindow::settle`] eases it back out.
pub const ENTER_SQUISH: Duration = Duration::from_millis(158);
const ENTER_SETTLE: f64 = 0.14;
const LEAVE_FADE: f64 = 0.12;
/// How long a going takes before [`NativeWindow::hide`] takes the window off screen: its fade, and a
/// frame more for AppKit to have finished it.
pub const LEAVE: Duration = Duration::from_millis(135);
const LEAVE_GROWTH: f64 = 0.08;

/// Changes made in `change` through a window's `animator()` take `seconds`, along `curve`. With no
/// seconds they are made at once, and replace an animation of the same values still running (set
/// directly, a value would be overwritten as that one ends).
fn animate(seconds: f64, curve: &CAMediaTimingFunctionName, change: impl FnOnce()) {
    NSAnimationContext::beginGrouping();
    let context = NSAnimationContext::currentContext();
    context.setDuration(seconds);
    context.setTimingFunction(Some(&CAMediaTimingFunction::functionWithName(curve)));
    change();
    NSAnimationContext::endGrouping();
}

/// Slow, fast, slow: a fade.
fn ease_in_out() -> &'static CAMediaTimingFunctionName {
    // SAFETY: Core Animation's constant, only read.
    unsafe { kCAMediaTimingFunctionEaseInEaseOut }
}

/// Fast, then slowing down to a stop: the narrowing as it comes.
fn ease_out() -> &'static CAMediaTimingFunctionName {
    // SAFETY: Core Animation's constant, only read.
    unsafe { kCAMediaTimingFunctionEaseOut }
}

/// Whether the user asked macOS for less motion (Accessibility → Display → Reduce motion).
fn reduce_motion() -> bool {
    NSWorkspace::sharedWorkspace().accessibilityDisplayShouldReduceMotion()
}

/// `rect` widened by `wide` of its width and heightened by `tall` of its height (below 0: made
/// smaller), about its centre.
fn grown(rect: NSRect, wide: f64, tall: f64) -> NSRect {
    let (width, height) = (rect.size.width * wide, rect.size.height * tall);
    NSRect::new(
        NSPoint::new(rect.origin.x - width / 2., rect.origin.y - height / 2.),
        NSSize::new(rect.size.width + width, rect.size.height + height),
    )
}

fn next(counter: &'static std::thread::LocalKey<Cell<u64>>) -> u64 {
    counter.with(|counter| {
        counter.set(counter.get() + 1);
        counter.get()
    })
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
        // While the launcher comes or goes, from where it rests.
        let frame = REST.get().unwrap_or_else(|| win.frame());
        let chrome = frame.size.height - win.contentRectForFrameRect(frame).size.height;
        let new_height = height + chrome;
        if (new_height - frame.size.height).abs() < 0.5 && (width - frame.size.width).abs() < 0.5 {
            return;
        }
        // The new size replaces what its coming or going was easing it toward.
        REST.set(None);
        next(&COMING);
        let top = frame.origin.y + frame.size.height;
        let x = frame.origin.x + (frame.size.width - width) / 2.;
        let new_frame = NSRect::new(NSPoint::new(x, top - new_height), NSSize::new(width, new_height));
        // Off screen, it just takes its size.
        if animate && win.isVisible() {
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

    /// Bring the launcher up as Spotlight comes (see [`ENTER_SQUISH`]): clear and wider than it
    /// rests, it fades in as it narrows past its width, and [`Self::settle`] eases it back out. It is
    /// presented as [`Self::present`] does. This coming's number, for [`Self::settle`].
    pub fn enter(&self) -> u64 {
        let win = &self.window;
        // Up already (another app has the keyboard): it only takes it back.
        if win.isVisible() && !LEAVING.get() {
            self.present();
            return next(&COMING);
        }
        // A going in progress stops here, and the window starts from where it rests.
        next(&GOING);
        LEAVING.set(false);
        let rest = REST.take().unwrap_or_else(|| win.frame());
        let coming = next(&COMING);
        let moving = !reduce_motion();
        let start = if moving { grown(rest, ENTER_WIDER, 0.) } else { rest };
        animate(0., ease_in_out(), || {
            win.animator().setFrame_display(start, true);
            win.animator().setAlphaValue(0.);
        });
        self.present();
        animate(ENTER_FADE, ease_in_out(), || win.animator().setAlphaValue(1.));
        if moving {
            REST.set(Some(rest));
            animate(ENTER_SQUISH.as_secs_f64(), ease_out(), || win.animator().setFrame_display(grown(rest, -ENTER_NARROWER, 0.), true));
        }
        coming
    }

    /// The end of [`Self::enter`]: from its narrowest, ease the launcher back out to its width, unless
    /// it came again, was resized or started going since.
    pub fn settle(&self, coming: u64) {
        if COMING.get() != coming || LEAVING.get() {
            return;
        }
        if let Some(rest) = REST.take() {
            animate(ENTER_SETTLE, ease_in_out(), || self.window.animator().setFrame_display(rest, true));
        }
    }

    /// Start taking the launcher away as Spotlight goes: it fades while it grows a little all round.
    /// [`Self::hide`] takes it off screen once that is done ([`LEAVE`]). This going's number.
    pub fn leave(&self) -> u64 {
        let win = &self.window;
        // Going already: the same going.
        if LEAVING.get() {
            return GOING.get();
        }
        let going = next(&GOING);
        LEAVING.set(true);
        let rest = REST.get().unwrap_or_else(|| win.frame());
        REST.set(Some(rest));
        animate(LEAVE_FADE, ease_in_out(), || win.animator().setAlphaValue(0.));
        if !reduce_motion() {
            animate(LEAVE_FADE, ease_in_out(), || win.animator().setFrame_display(grown(rest, LEAVE_GROWTH, LEAVE_GROWTH), true));
        }
        going
    }

    /// Whether the launcher is fading away: it is gone as far as the hotkey is concerned.
    pub fn is_leaving(&self) -> bool {
        LEAVING.get()
    }

    /// Whether the window has the keyboard.
    pub fn is_key(&self) -> bool {
        self.window.isKeyWindow()
    }

    /// Whether keys go to the window: it is key, and Delight is the active app.
    pub fn has_keyboard(&self) -> bool {
        let Some(mtm) = main_thread() else { return false };
        self.window.isKeyWindow() && NSApplication::sharedApplication(mtm).isActive()
    }

    /// Bring the window in front of the others at its level, when it is clicked. Call it outside
    /// a GPUI update, like [`Self::present`]: AppKit calls back into GPUI.
    pub fn raise(&self) {
        self.window.orderFront(None);
    }

    /// How far in front of others the window sits (AppKit's window level).
    pub fn level(&self) -> isize {
        self.window.level()
    }

    pub fn set_level(&self, level: isize) {
        self.window.setLevel(level);
    }

    /// Take the window off screen without closing it, leaving the keyboard and the app in front
    /// alone: a plugin's window, hidden with the launcher, or the launcher giving way to Settings.
    pub fn order_out(&self) {
        self.window.orderOut(None);
    }

    /// Put a window hidden with [`Self::order_out`] back where it was, behind the key window.
    pub fn order_front(&self) {
        self.window.orderFrontRegardless();
    }

    /// Once [`Self::leave`] is done, take the window off screen without closing it, so all state
    /// survives, and put it back as it rests for the next time: whether it went (not if it came
    /// again, or started going again, since). With `back_to_previous`, if the launcher had the
    /// keyboard (hidden by Esc or the hotkey), the app that was in front before comes back; not if
    /// another app was clicked, nor when another of Delight's windows takes its place.
    pub fn hide(&self, going: u64, back_to_previous: bool) -> bool {
        if GOING.get() != going {
            return false;
        }
        LEAVING.set(false);
        let had_keyboard = self.has_keyboard();
        let win = &self.window;
        win.orderOut(None);
        let rest = REST.take();
        animate(0., ease_in_out(), || {
            if let Some(rest) = rest {
                win.animator().setFrame_display(rest, false);
            }
            win.animator().setAlphaValue(1.);
        });
        if back_to_previous
            && let Some(previous) = PREVIOUS_APP.take()
            && had_keyboard
        {
            previous.activateWithOptions(NSApplicationActivationOptions::empty());
        }
        true
    }
}

thread_local! {
    /// Made once: making a date formatter is slow, and a page draws its dates on every frame.
    static DATE_FORMATTER: Retained<objc2_foundation::NSDateFormatter> = {
        use objc2_foundation::{NSDateFormatter, NSDateFormatterStyle};
        let formatter = NSDateFormatter::new();
        formatter.setDateStyle(NSDateFormatterStyle::MediumStyle);
        formatter.setTimeStyle(NSDateFormatterStyle::ShortStyle);
        formatter.setDoesRelativeDateFormatting(true);
        formatter
    };
}

/// `time` as macOS writes a date and time, in the user's language and settings (12 or 24 hours),
/// as a word when it can: "Today at 4:34 PM", "Yesterday at 09:12", "29 Sep 2026 at 16:34".
pub fn date_and_time(time: std::time::SystemTime) -> String {
    let seconds = time.duration_since(std::time::UNIX_EPOCH).map_or(0., |since| since.as_secs_f64());
    let date = objc2_foundation::NSDate::dateWithTimeIntervalSince1970(seconds);
    DATE_FORMATTER.with(|formatter| formatter.stringFromDate(&date).to_string())
}

/// Show `path` in Finder: a folder is opened, a file is selected in its folder.
pub fn reveal(path: &std::path::Path) {
    use objc2_foundation::{NSArray, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let workspace = NSWorkspace::sharedWorkspace();
    if path.is_dir() {
        workspace.openURL(&url);
    } else {
        workspace.activateFileViewerSelectingURLs(&NSArray::from_slice(&[&*url]));
    }
}

/// Open the file in the Mac's text editor: the app that opens a plain `.txt` file (TextEdit unless
/// the user chose another). The file's own default app (for `.log`, Console) if none is found.
pub fn open_in_text_editor(path: &std::path::Path) {
    use objc2_app_kit::NSWorkspaceOpenConfiguration;
    use objc2_foundation::{NSArray, NSString, NSURL};
    let url = NSURL::fileURLWithPath(&NSString::from_str(&path.to_string_lossy()));
    let workspace = NSWorkspace::sharedWorkspace();
    // Asked by name only: what opens a `.txt` file doesn't need the file to exist.
    let text_file = NSURL::fileURLWithPath(&NSString::from_str("/tmp/delight.txt"));
    match workspace.URLForApplicationToOpenURL(&text_file) {
        Some(editor) => workspace.openURLs_withApplicationAtURL_configuration_completionHandler(
            &NSArray::from_slice(&[&*url]),
            &editor,
            &NSWorkspaceOpenConfiguration::configuration(),
            None,
        ),
        None => {
            workspace.openURL(&url);
        }
    }
}
