//! What the app hands a plugin beyond its sandbox, as objects, so that holding one is
//! the authority to use it: its clipboard, and (later) what its permissions grant.
//! (The network is the sandbox's own: see [`plugin_options`](crate::plugin_options).)

use embedded_gpui::gpui::{App, Entity};
use embedded_gpui::{Clipboard, ClipboardApi, Ref, Registry};

/// The objects a plugin is given, made when it starts
/// ([`Plugin::start`](crate::Plugin::start)) for the app's root object to hand out.
///
/// Each is shared anew whenever the plugin asks for it: a ref lasts until the plugin
/// drops what it connected. Objects for permissions (running programs) come with the
/// plugins that need them (docs/plan.md, step 14), made here only when the manifest
/// asks for the permission.
pub struct Granted {
    /// The plugin's end of the connection, to share them through.
    registry: Registry,
    /// embedded_gpui's clipboard object for this plugin: it reads and writes the Mac's
    /// clipboard, and looks at it again before each ⌘ key it forwards to the plugin,
    /// so ⌘V in a plugin's text field pastes what's there.
    clipboard: Entity<Clipboard>,
}

impl Granted {
    pub(crate) fn new(registry: Registry, clipboard: Entity<Clipboard>) -> Self {
        Granted { registry, clipboard }
    }

    /// The clipboard, for GPUI's own clipboard calls in the plugin.
    pub fn clipboard(&self, cx: &mut App) -> Ref<ClipboardApi> {
        self.registry.share(&self.clipboard, cx)
    }
}
