//! DNS: type a host name, URL or IP address to see its DNS records, the resolver that
//! answered, what the system resolver (what apps get) returns, and reverse lookups.
//!
//! * this file: the plugin, and which inputs it looks up.
//! * `lookup`: what to look up, which resolver answers it, and the report, without
//!   UI or network.
//! * `query`: asking: the Mac's resolvers from the app, DNS queries over UDP, the
//!   system lookup through WASI.
//! * `view`: the tool.

mod lookup;
mod query;
mod view;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};

use crate::lookup::Target;

#[plugin(
    id = "delight.dns",
    name = "DNS",
    description = "Look up a host's DNS records, resolver and addresses.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["dns", "network", "host"],
    permissions = [Network("Asks your DNS servers directly, as dig does")],
    tips = ["Type a host name or IP address to look it up"],
)]
struct Dns;

#[derive(Operations)]
enum DnsOperation {
    #[operation(
        id = "lookup",
        title = "DNS lookup",
        description = "DNS records, resolver and reverse lookup for a host",
        tags = ["dns", "host", "nslookup", "dig", "ip"],
    )]
    Lookup,
}

impl Plugin for Dns {
    type Operation = DnsOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Dns
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<DnsOperation>> {
        confidence(&input.text).map(|confidence| Detection::new(DnsOperation::Lookup, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: DnsOperation, cx: &mut App) -> AnyTool {
        match operation {
            DnsOperation::Lookup => cx.new(|_| view::DnsView::default()).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}

/// How surely `text` is something to look up: an address, a host name, or a URL
/// (lower, as a URL is more likely something else's).
fn confidence(text: &str) -> Option<f32> {
    let (target, is_url) = lookup::target(text)?;
    Some(match target {
        Target::Ip(_) => 0.8,
        Target::Host(_) if is_url => 0.45,
        Target::Host(_) => 0.75,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_hosts_urls_and_addresses() {
        assert_eq!(confidence("api.example.com"), Some(0.75));
        assert_eq!(confidence("https://api.example.com/v1"), Some(0.45));
        assert_eq!(confidence("10.1.2.3"), Some(0.8));
        assert_eq!(confidence("hello"), None);
    }
}
