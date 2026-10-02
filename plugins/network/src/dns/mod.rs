//! DNS: type a host name, URL or IP address to see its DNS records, the resolver that
//! answered, what the system resolver (what apps get) returns, and reverse lookups.
//!
//! * this file: which inputs it looks up.
//! * `lookup`: what to look up, which resolver answers it, and the report, without
//!   UI or network.
//! * `query`: asking: the Mac's resolvers from the app, DNS queries over UDP, the
//!   system lookup through WASI.
//! * `view`: the tool.

mod lookup;
mod query;
mod view;

use lookup::Target;

pub use view::DnsView;

/// How surely `text` is something to look up: an address, a host name, or a URL
/// (lower, as a URL is more likely something else's).
pub fn confidence(text: &str) -> Option<f32> {
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
