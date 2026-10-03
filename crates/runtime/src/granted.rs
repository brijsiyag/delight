//! What the app hands a plugin beyond its sandbox, as objects, so that holding one is
//! the authority to use it: its clipboard, and what its permissions grant (HTTP, for
//! `Network`, running programs for `Commands`). A `Network` plugin also has the sandbox's own sockets: see
//! [`plugin_options`](crate::plugin_options).

use delight_manifest::{CommandsPermission, NetworkPermission, Permissions};
use std::path::PathBuf;

use delight_protocol::{CommandsApi, DnsApi, HttpApi};
use embedded_gpui::gpui::{App, AppContext as _, Entity};
use embedded_gpui::{Clipboard, ClipboardApi, Ref, Registry};

use crate::commands::Commands;
use crate::dns::Dns;
// TEMPORARY(network)
use crate::network::Http;

/// The objects a plugin is given, made when it starts
/// ([`Plugin::start`](crate::Plugin::start)) for the app's root object to hand out.
///
/// Each is shared anew whenever the plugin asks for it: a ref lasts until the plugin
/// drops what it connected. An object for a permission is made here only when the
/// plugin has the permission.
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
    /// With [`Permission::Commands`]: the programs it lists.
    commands: Option<Entity<Commands>>,
}

impl Granted {
    pub(crate) fn new(
        permissions: &Permissions,
        data_dir: PathBuf,
        registry: Registry,
        clipboard: Entity<Clipboard>,
        cx: &mut App,
    ) -> Self {
        // TEMPORARY(network)
        let network = permissions.get::<NetworkPermission>().is_some();
        let http = network.then(|| cx.new(|_| Http::new(registry.clone())));
        let dns = network.then(|| cx.new(|_| Dns));
        let commands = permissions.get::<CommandsPermission>().map(|commands| cx.new(|_| Commands::new(&commands.programs, data_dir)));
        Granted { registry, clipboard, http, dns, commands }
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

    /// Running the programs it may run, if the plugin has [`Permission::Commands`].
    pub fn commands(&self, cx: &mut App) -> Option<Ref<CommandsApi>> {
        let commands = self.commands.as_ref()?;
        Some(self.registry.share(commands, cx))
    }
}
