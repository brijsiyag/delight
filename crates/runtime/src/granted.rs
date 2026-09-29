//! What the app hands a plugin beyond its sandbox, as objects, so that holding one is
//! the authority to use it: its clipboard, and what its permissions grant (HTTP, for
//! `Network`). A `Network` plugin also has the sandbox's own sockets: see
//! [`plugin_options`](crate::plugin_options).

use delight_manifest::{Manifest, Permission};
use delight_protocol::{DnsApi, HttpApi};
use embedded_gpui::gpui::{App, AppContext as _, Entity};
use embedded_gpui::{Clipboard, ClipboardApi, Ref, Registry};

use crate::dns::Dns;
// TEMPORARY(network)
use crate::network::Http;

/// The objects a plugin is given, made when it starts
/// ([`Plugin::start`](crate::Plugin::start)) for the app's root object to hand out.
///
/// Each is shared anew whenever the plugin asks for it: a ref lasts until the plugin
/// drops what it connected. An object for a permission is made here only when the
/// manifest asks for the permission.
pub struct Granted {
    /// The plugin's end of the connection, to share them through.
    registry: Registry,
    /// embedded_gpui's clipboard object for this plugin: it reads and writes the Mac's
    /// clipboard, and looks at it again before each ⌘ key it forwards to the plugin,
    /// so ⌘V in a plugin's text field pastes what's there.
    clipboard: Entity<Clipboard>,
    /// With [`Permission::Network`].
    // TEMPORARY(network)
    http: Option<Entity<Http>>,
    /// With [`Permission::Network`]: the Mac's DNS setup.
    dns: Option<Entity<Dns>>,
}

impl Granted {
    pub(crate) fn new(manifest: &Manifest, registry: Registry, clipboard: Entity<Clipboard>, cx: &mut App) -> Self {
        // TEMPORARY(network)
        let http = manifest.plugin.asks_for(Permission::Network).then(|| cx.new(|_| Http::new(registry.clone())));
        let dns = manifest.plugin.asks_for(Permission::Network).then(|| cx.new(|_| Dns));
        Granted { registry, clipboard, http, dns }
    }

    /// The clipboard, for GPUI's own clipboard calls in the plugin.
    pub fn clipboard(&self, cx: &mut App) -> Ref<ClipboardApi> {
        self.registry.share(&self.clipboard, cx)
    }

    /// HTTP and listeners, if the plugin has [`Permission::Network`].
    // TEMPORARY(network)
    pub fn http(&self, cx: &mut App) -> Option<Ref<HttpApi>> {
        let http = self.http.as_ref()?;
        Some(self.registry.share(http, cx))
    }

    /// The Mac's DNS setup, if the plugin has [`Permission::Network`].
    pub fn dns(&self, cx: &mut App) -> Option<Ref<DnsApi>> {
        let dns = self.dns.as_ref()?;
        Some(self.registry.share(dns, cx))
    }
}
