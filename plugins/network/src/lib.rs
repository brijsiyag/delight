//! Network: tools for hosts and addresses. For now one, DNS lookup.
//!
//! * this file: the plugin and its tools.
//! * `dns`: a host's DNS records, the resolver that answered, what apps get, and reverse
//!   lookups.

mod dns;

use delight_plugin_api::{AnyTool, Detection, Input, Operations, Plugin, plugin};
use gpui::{App, AppContext as _, AssetSource};

#[plugin(
    id = "delight_network",
    name = "Network",
    description = "Look up hosts and addresses: DNS records, resolvers, reverse lookups.",
    author = "Delight",
    icon = "assets/icon.svg",
    tags = ["network", "dns", "host"],
    permissions = [Network("Asks your DNS servers directly, as dig does")],
    tips = ["Type a host name or IP address to look it up"],
)]
struct Network;

#[derive(Operations)]
enum NetworkOperation {
    #[operation(
        id = "lookup",
        title = "DNS lookup",
        description = "DNS records, resolver and reverse lookup for a host",
        icon = "assets/dns.svg",
        tags = ["dns", "host", "nslookup", "dig", "ip"],
    )]
    Lookup,
}

impl Plugin for Network {
    type Operation = NetworkOperation;

    fn new(cx: &mut App) -> Self {
        delight_ui::init_plugin(cx);
        Network
    }

    fn detect(&mut self, input: &Input, _cx: &mut App) -> Vec<Detection<NetworkOperation>> {
        dns::confidence(&input.text).map(|confidence| Detection::new(NetworkOperation::Lookup, confidence)).into_iter().collect()
    }

    fn open_tool(&mut self, operation: NetworkOperation, cx: &mut App) -> AnyTool {
        match operation {
            NetworkOperation::Lookup => cx.new(|_| dns::DnsView::default()).into(),
        }
    }

    fn assets() -> Option<Box<dyn AssetSource>> {
        Some(Box::new(delight_ui::Assets))
    }
}
