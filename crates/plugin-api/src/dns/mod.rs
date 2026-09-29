//! The Mac's DNS setup, which a plugin's sandbox can't see ([`Host::dns_resolvers`]).

use anyhow::{Context as _, Result, anyhow};
use delight_protocol::{DnsApiCaller as _, DnsResolver, HostApiCaller as _};

use crate::Host;
use crate::gpui::{App, Task};

impl Host {
    /// The resolvers macOS uses now, the default one first: which servers answer, for
    /// which domains (a VPN's among them), and the search domains. Needs the
    /// `Network` permission.
    pub fn dns_resolvers(&self, cx: &mut App) -> Task<Result<Vec<DnsResolver>>> {
        let Some(remote) = self.remote.clone() else {
            return Task::ready(Err(anyhow!("the Mac's DNS setup is Delight's: a plugin reads it only in Delight")));
        };
        let asked = remote.dns(cx);
        cx.spawn(async move |cx| {
            let dns = asked.await?.context("this plugin doesn't have the Network permission")?.connect();
            let resolvers = cx.update(|cx| dns.dns_resolvers(cx));
            resolvers.await
        })
    }
}
