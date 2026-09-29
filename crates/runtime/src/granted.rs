//! What a plugin's permissions give it beyond its sandbox, as objects: the app hands
//! a plugin only the ones it was granted, so holding one is the permission. (The
//! network is the sandbox's own: see [`plugin_options`](crate::plugin_options).)

use embedded_gpui::Registry;

/// The objects a plugin's manifest grants it, made when it starts
/// ([`Plugin::start`](crate::Plugin::start)) for the app's root object to hand out,
/// each from a `HostApi` method that answers `None` without the permission.
///
/// None yet: reading the clipboard and running programs come with the plugins that
/// need them (docs/plan.md, step 14). Each is an entity made here only when the
/// manifest asks for its permission, and shared anew whenever the plugin asks for it
/// (a ref lasts until the plugin drops what it connected).
pub struct Granted {
    /// The plugin's end of the connection, to share them through.
    #[expect(dead_code, reason = "no permission has an object yet")]
    registry: Registry,
}

impl Granted {
    pub(crate) fn new(registry: Registry) -> Self {
        Granted { registry }
    }
}
