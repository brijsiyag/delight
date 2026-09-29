//! The Mac's DNS setup for plugins ([`DnsApi`]), read as `scutil --dns` reads it but
//! without running it: macOS's configuration store (the default resolver, and the
//! ones a VPN or other service adds for its own domains), and the `/etc/resolver/`
//! files some tools add for theirs. `/etc/resolv.conf` has only the default.

use std::path::Path;

use anyhow::{Result, anyhow};
use delight_protocol::{DnsApi, DnsResolver};
use embedded_gpui::gpui::{AppContext as _, Context, Task};
use embedded_gpui::shared;
use system_configuration::core_foundation::array::CFArray;
use system_configuration::core_foundation::base::{CFType, TCFType};
use system_configuration::core_foundation::dictionary::CFDictionary;
use system_configuration::core_foundation::string::CFString;
use system_configuration::dynamic_store::{SCDynamicStore, SCDynamicStoreBuilder};

/// Where tools such as VPN clients put a resolver for one domain, a file named after it.
const RESOLVER_FILES: &str = "/etc/resolver";

pub(crate) struct Dns;

#[shared]
impl DnsApi for Dns {
    fn dns_resolvers(&mut self, cx: &mut Context<Self>) -> Task<Result<Vec<DnsResolver>>> {
        cx.background_spawn(async move {
            let store = SCDynamicStoreBuilder::new("Delight")
                .build()
                .ok_or_else(|| anyhow!("macOS's configuration store isn't available"))?;
            let default = dns_entry(&store, "State:/Network/Global/DNS");
            let services = store
                .get_keys("State:/Network/Service/.*/DNS")
                .map(|keys| keys.iter().filter_map(|key| dns_entry(&store, &key.to_string())).collect())
                .unwrap_or_default();
            Ok(resolvers(default, services, resolver_files(Path::new(RESOLVER_FILES))))
        })
    }
}

/// One DNS entry of macOS's configuration store, as plain lists.
#[derive(Debug, Default, PartialEq)]
struct Entry {
    servers: Vec<String>,
    search: Vec<String>,
    /// The domains it answers for (a VPN's); none for the default resolver.
    domains: Vec<String>,
}

/// The DNS entry at `key`, if there's one.
fn dns_entry(store: &SCDynamicStore, key: &str) -> Option<Entry> {
    let dictionary = store.get(key)?.downcast_into::<CFDictionary>()?;
    Some(Entry {
        servers: strings(&dictionary, "ServerAddresses"),
        search: strings(&dictionary, "SearchDomains"),
        domains: strings(&dictionary, "SupplementalMatchDomains"),
    })
}

/// The strings in an entry's array under `key`.
fn strings(dictionary: &CFDictionary, key: &'static str) -> Vec<String> {
    let key = CFString::from_static_string(key);
    let Some(value) = dictionary.find(key.as_CFTypeRef()) else {
        return Vec::new();
    };
    // SAFETY: the dictionary holds the value, and this takes its own reference to it.
    let value = unsafe { CFType::wrap_under_get_rule(*value) };
    let Some(array) = value.downcast::<CFArray>() else {
        return Vec::new();
    };
    array
        .iter()
        .filter_map(|item| {
            // SAFETY: as above, for the array's item.
            let item = unsafe { CFType::wrap_under_get_rule(*item) };
            item.downcast::<CFString>().map(|string| string.to_string())
        })
        .collect()
}

/// The `/etc/resolver/<domain>` files: each domain, and its file's settings.
fn resolver_files(folder: &Path) -> Vec<(String, resolv_conf::Config)> {
    let Ok(files) = std::fs::read_dir(folder) else { return Vec::new() };
    files
        .flatten()
        .filter_map(|file| {
            let domain = file.file_name().to_str()?.to_string();
            let contents = std::fs::read(file.path()).ok()?;
            let config = resolv_conf::Config::parse(contents).ok()?;
            (!domain.starts_with('.')).then_some((domain, config))
        })
        .collect()
}

/// The resolvers, the default first, then one per domain another answers for (from
/// services, then files); only those with servers.
fn resolvers(default: Option<Entry>, services: Vec<Entry>, files: Vec<(String, resolv_conf::Config)>) -> Vec<DnsResolver> {
    let domain = |domain: &str| domain.trim_end_matches('.').to_ascii_lowercase();
    let mut resolvers: Vec<DnsResolver> = default
        .map(|entry| DnsResolver { domain: None, nameservers: entry.servers, search: entry.search })
        .into_iter()
        .collect();
    for entry in services {
        for scoped in entry.domains.iter().map(|name| domain(name)).filter(|name| !name.is_empty()) {
            resolvers.push(DnsResolver { domain: Some(scoped), nameservers: entry.servers.clone(), search: Vec::new() });
        }
    }
    for (name, config) in files {
        let nameservers = config.nameservers.iter().map(ToString::to_string).collect();
        resolvers.push(DnsResolver { domain: Some(domain(&name)), nameservers, search: Vec::new() });
    }
    resolvers.retain(|resolver| !resolver.nameservers.is_empty());
    resolvers.dedup();
    resolvers
}

#[cfg(test)]
mod tests {
    use super::*;

    fn resolver(domain: Option<&str>, server: &str, search: &[&str]) -> DnsResolver {
        DnsResolver {
            domain: domain.map(Into::into),
            nameservers: vec![server.into()],
            search: search.iter().map(|&name| name.into()).collect(),
        }
    }

    #[test]
    fn the_default_first_then_each_domains_own() {
        let default = Entry { servers: vec!["10.0.0.1".into()], search: vec!["corp.example".into()], domains: Vec::new() };
        let vpn = Entry { servers: vec!["172.16.0.2".into()], search: Vec::new(), domains: vec!["Corp.Example.".into(), "".into()] };
        // The primary service's own entry repeats the default, without domains.
        let primary = Entry { servers: vec!["10.0.0.1".into()], search: Vec::new(), domains: Vec::new() };
        let file = resolv_conf::Config::parse("nameserver 100.100.100.100\n").unwrap();
        let found = resolvers(Some(default), vec![primary, vpn], vec![("ts.net".into(), file)]);
        assert_eq!(
            found,
            [
                resolver(None, "10.0.0.1", &["corp.example"]),
                resolver(Some("corp.example"), "172.16.0.2", &[]),
                resolver(Some("ts.net"), "100.100.100.100", &[]),
            ]
        );
    }

    #[test]
    fn an_entry_reads_from_its_dictionary() {
        let servers = CFArray::from_CFTypes(&[CFString::new("10.0.0.1"), CFString::new("10.0.0.2")]);
        let dictionary = CFDictionary::from_CFType_pairs(&[(CFString::new("ServerAddresses"), servers.as_CFType())]);
        let untyped = dictionary.to_untyped();
        assert_eq!(strings(&untyped, "ServerAddresses"), ["10.0.0.1", "10.0.0.2"]);
        assert!(strings(&untyped, "SearchDomains").is_empty());
    }

    #[test]
    fn the_macs_own_setup_reads() {
        // The real store: whatever it holds, reading it works.
        let store = SCDynamicStoreBuilder::new("Delight tests").build().expect("the store");
        let _ = dns_entry(&store, "State:/Network/Global/DNS");
    }
}
