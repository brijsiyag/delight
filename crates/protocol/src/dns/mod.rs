//! The Mac's DNS setup, which a plugin's sandbox can't see: which servers answer, for
//! which domains (a VPN's among them), and the search domains. For plugins with the
//! `Network` permission.

use embedded_gpui::{data, interface};

/// The Mac's DNS setup, homed in the app.
#[interface]
pub trait DnsApi {
    /// The resolvers macOS uses now, the default one first.
    async fn dns_resolvers(&mut self, cx: &mut gpui::Context<Self>) -> Vec<DnsResolver>;
}

/// One of macOS's resolvers.
#[data]
#[derive(Default, PartialEq)]
pub struct DnsResolver {
    /// The domain it answers for (`corp.example` for a VPN's); `None` for the
    /// default resolver.
    pub domain: Option<String>,
    /// Its servers' addresses, in the order macOS tries them.
    pub nameservers: Vec<String>,
    /// The domains a short name is tried in.
    pub search: Vec<String>,
}
