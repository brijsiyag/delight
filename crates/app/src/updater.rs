//! Updates, by [Sparkle](https://sparkle-project.org): checked once a day, and from the menu bar
//! icon's "Check for Updates…" and General. Sparkle downloads, verifies (an EdDSA signature
//! against `SUPublicEDKey`) and installs, with its own windows.
//!
//! Sparkle is loaded when the app starts, from `Contents/Frameworks/Sparkle.framework`, so a
//! build outside a `.app` (`cargo run`) has no updater (the menu item is there, dimmed). Its
//! delegate reports what it found to this module, which keeps it as a [`Status`] for the
//! menu and General.
//!
//! The feed, the public key and the daily check are the bundle's `Info.plist` (`SUFeedURL`,
//! `SUPublicEDKey`, `SUEnableAutomaticChecks`, `SUScheduledCheckInterval`), as the previous
//! attempt's were; the release task writes it (see `docs/plan.md`, step 15).

use std::path::Path;

use futures::StreamExt as _;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::{App, Global};
use objc2::rc::{Allocated, Retained};
use objc2::runtime::{AnyClass, AnyObject, NSObject, NSObjectProtocol};
use objc2::{AnyThread, DefinedClass, define_class, msg_send};
use objc2_foundation::NSString;

/// What the updater last found.
#[derive(Clone, Debug, PartialEq)]
pub enum Status {
    /// Nothing asked yet.
    Idle,
    /// A check is running.
    Checking,
    /// A newer version exists (its version). Sparkle offers to install it.
    Found(String),
    /// This is the newest.
    UpToDate,
    /// The check failed (why).
    Failed(String),
}

enum Event {
    Found(String),
    UpToDate,
    Failed(String),
}

/// What Sparkle's delegate holds: where to report.
struct Reports(UnboundedSender<Event>);

define_class!(
    // SAFETY: NSObject has no subclassing requirements, and this has no `Drop`.
    #[unsafe(super(NSObject))]
    #[name = "DelightUpdaterDelegate"]
    #[ivars = Reports]
    struct Delegate;

    unsafe impl NSObjectProtocol for Delegate {}

    // Sparkle's `SPUUpdaterDelegate`: it asks which of these an object answers, so none of
    // them needs declaring.
    impl Delegate {
        #[unsafe(method(updater:didFindValidUpdate:))]
        fn found(&self, _updater: &AnyObject, item: &AnyObject) {
            let version: Option<Retained<NSString>> = unsafe { msg_send![item, displayVersionString] };
            let version = version.map_or_else(|| "a newer version".to_string(), |version| version.to_string());
            self.ivars().0.unbounded_send(Event::Found(version)).ok();
        }

        // Sparkle 2's form, which carries why (nothing newer, mostly).
        #[unsafe(method(updaterDidNotFindUpdate:error:))]
        fn not_found(&self, _updater: &AnyObject, _error: &AnyObject) {
            self.ivars().0.unbounded_send(Event::UpToDate).ok();
        }

        #[unsafe(method(updater:didAbortWithError:))]
        fn aborted(&self, _updater: &AnyObject, error: &AnyObject) {
            let why: Option<Retained<NSString>> = unsafe { msg_send![error, localizedDescription] };
            let why = why.map_or_else(|| "the update check failed".to_string(), |why| why.to_string());
            self.ivars().0.unbounded_send(Event::Failed(why)).ok();
        }
    }
);

impl Delegate {
    fn new(reports: UnboundedSender<Event>) -> Retained<Self> {
        let this = Self::alloc().set_ivars(Reports(reports));
        unsafe { msg_send![super(this), init] }
    }
}

/// Sparkle's controller (which Sparkle keeps running), and what it last said.
struct Updater {
    controller: Retained<AnyObject>,
    /// Sparkle holds its delegate weakly: this keeps it.
    _delegate: Retained<Delegate>,
    status: Status,
}

impl Global for Updater {}

/// Start Sparkle if this is a `.app` that has it. Call it once, on the main thread, before
/// the tray is built.
pub fn init(cx: &mut App) {
    let Some(framework) = framework() else {
        log::info!("no Sparkle next to this build: updates are off");
        return;
    };
    match start(&framework) {
        Ok((controller, delegate, events)) => {
            cx.set_global(Updater { controller, _delegate: delegate, status: Status::Idle });
            let mut events = events;
            cx.spawn(async move |cx| {
                while let Some(event) = events.next().await {
                    cx.update(|cx| {
                        set(
                            match event {
                                Event::Found(version) => Status::Found(version),
                                Event::UpToDate => Status::UpToDate,
                                Event::Failed(why) => Status::Failed(why),
                            },
                            cx,
                        )
                    });
                }
            })
            .detach();
        }
        Err(error) => log::error!("starting the updater: {error:#}"),
    }
}

/// Whether this build can update itself: it is a `.app` with Sparkle, started.
pub fn available(cx: &App) -> bool {
    cx.has_global::<Updater>()
}

pub fn status(cx: &App) -> Status {
    cx.try_global::<Updater>().map_or(Status::Idle, |updater| updater.status.clone())
}

/// Look for an update now: Sparkle shows its own windows for what it finds.
pub fn check(cx: &mut App) {
    let Some(updater) = cx.try_global::<Updater>() else { return };
    let controller = updater.controller.clone();
    set(Status::Checking, cx);
    // SAFETY: `checkForUpdates:` takes the sender, which may be nil.
    let _: () = unsafe { msg_send![&*controller, checkForUpdates: std::ptr::null::<AnyObject>()] };
}

fn set(status: Status, cx: &mut App) {
    if cx.has_global::<Updater>() {
        cx.global_mut::<Updater>().status = status.clone();
    }
    crate::tray::set_update_status(&status, cx);
    cx.refresh_windows();
}

/// `Sparkle.framework` next to the executable, in `Delight.app/Contents/Frameworks`.
fn framework() -> Option<std::path::PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let contents = exe.parent()?.parent()?;
    let framework = contents.join("Frameworks/Sparkle.framework");
    (contents.file_name()? == "Contents" && framework.is_dir()).then_some(framework)
}

fn start(framework: &Path) -> anyhow::Result<(Retained<AnyObject>, Retained<Delegate>, futures::channel::mpsc::UnboundedReceiver<Event>)> {
    // Load the framework, which registers its classes.
    let bundle_class = AnyClass::get(c"NSBundle").ok_or_else(|| anyhow::anyhow!("no NSBundle"))?;
    let path = NSString::from_str(&framework.to_string_lossy());
    let bundle: Option<Retained<AnyObject>> = unsafe { msg_send![bundle_class, bundleWithPath: &*path] };
    let bundle = bundle.ok_or_else(|| anyhow::anyhow!("{} isn't a bundle", framework.display()))?;
    let loaded: bool = unsafe { msg_send![&*bundle, load] };
    anyhow::ensure!(loaded, "{} didn't load", framework.display());

    let class = AnyClass::get(c"SPUStandardUpdaterController")
        .ok_or_else(|| anyhow::anyhow!("Sparkle has no SPUStandardUpdaterController"))?;
    let (reports, events) = unbounded();
    let delegate = Delegate::new(reports);
    // SAFETY: `initWithStartingUpdater:updaterDelegate:userDriverDelegate:` is Sparkle's
    // designated initialiser; the user driver delegate may be nil.
    let controller: Option<Retained<AnyObject>> = unsafe {
        let allocated: Allocated<AnyObject> = msg_send![class, alloc];
        msg_send![
            allocated,
            initWithStartingUpdater: true,
            updaterDelegate: &*delegate,
            userDriverDelegate: std::ptr::null::<AnyObject>()
        ]
    };
    let controller = controller.ok_or_else(|| anyhow::anyhow!("Sparkle's controller didn't start"))?;
    Ok((controller, delegate, events))
}

#[cfg(test)]
mod tests {
    use super::*;
    use objc2::ClassType as _;

    #[test]
    fn a_build_outside_an_app_has_no_updater() {
        // `cargo test` runs from target/debug/deps, not from Delight.app/Contents/MacOS.
        assert!(framework().is_none());
    }

    #[test]
    fn the_delegate_reports_what_sparkle_finds() {
        let (reports, mut events) = unbounded();
        let delegate = Delegate::new(reports);
        let nothing = std::ptr::null::<AnyObject>();

        // As Sparkle calls it: by selector, on the object.
        let _: () = unsafe { msg_send![&*delegate, updaterDidNotFindUpdate: nothing, error: nothing] };
        assert!(matches!(events.try_recv(), Ok(Event::UpToDate)));

        // An update: Sparkle passes its `SUAppcastItem`, which answers `displayVersionString`.
        let item: Retained<FakeItem> = unsafe { msg_send![FakeItem::class(), new] };
        let _: () = unsafe { msg_send![&*delegate, updater: nothing, didFindValidUpdate: &*item] };
        assert!(matches!(events.try_recv(), Ok(Event::Found(version)) if version == "2.1.0"));

        let error: Retained<FakeError> = unsafe { msg_send![FakeError::class(), new] };
        let _: () = unsafe { msg_send![&*delegate, updater: nothing, didAbortWithError: &*error] };
        assert!(matches!(events.try_recv(), Ok(Event::Failed(why)) if why == "no network"));
    }

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and this has no `Drop`.
        #[unsafe(super(NSObject))]
        #[name = "DelightFakeAppcastItem"]
        struct FakeItem;

        unsafe impl NSObjectProtocol for FakeItem {}

        impl FakeItem {
            #[unsafe(method_id(displayVersionString))]
            fn display_version_string(&self) -> Retained<NSString> {
                NSString::from_str("2.1.0")
            }
        }
    );

    define_class!(
        // SAFETY: NSObject has no subclassing requirements, and this has no `Drop`.
        #[unsafe(super(NSObject))]
        #[name = "DelightFakeError"]
        struct FakeError;

        unsafe impl NSObjectProtocol for FakeError {}

        impl FakeError {
            #[unsafe(method_id(localizedDescription))]
            fn localized_description(&self) -> Retained<NSString> {
                NSString::from_str("no network")
            }
        }
    );
}
