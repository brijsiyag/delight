//! A DNS lookup, without UI or network: what to look up in the input, which of
//! macOS's resolvers answers it (a VPN's for its domains), the answers, and the
//! report drawn from them.

use std::net::IpAddr;

use delight_plugin_api::DnsResolver as Resolver;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
pub enum Target {
    Host(String),
    Ip(IpAddr),
}

fn is_hostname(s: &str) -> bool {
    let labels: Vec<&str> = s.trim_end_matches('.').split('.').collect();
    labels.len() >= 2
        && labels.iter().all(|l| {
            !l.is_empty()
                && l.len() <= 63
                && !l.starts_with('-')
                && !l.ends_with('-')
                && l.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
        })
        && labels.last().is_some_and(|tld| tld.len() >= 2 && tld.chars().all(|c| c.is_ascii_alphabetic()))
}

/// The host in `input` — a bare hostname or IP, `host:port`, or a URL — and
/// whether it was a URL.
pub fn target(input: &str) -> Option<(Target, bool)> {
    let t = input.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return None;
    }
    let is_url = t.contains("://");
    let rest = t.split_once("://").map_or(t, |(_, r)| r);
    let authority = rest.split(['/', '?', '#']).next()?;
    let host = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
    // [v6]:port, v6, host:port, host
    let host = if let Some(v6) = host.strip_prefix('[') {
        v6.split(']').next()?
    } else if host.matches(':').count() == 1 {
        host.split(':').next()?
    } else {
        host
    };
    if let Ok(ip) = host.parse::<IpAddr>() {
        return Some((Target::Ip(ip), is_url));
    }
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    is_hostname(&host).then_some((Target::Host(host), is_url))
}

// ---------------------------------------------------------------------------
// Resolvers (from the app: `host(cx).dns_resolvers`)
// ---------------------------------------------------------------------------

/// The resolver macOS uses for `host`: the longest matching scoped domain,
/// otherwise the default (first unscoped) one.
pub fn resolver_for<'a>(resolvers: &'a [Resolver], host: &str) -> Option<&'a Resolver> {
    let scoped = resolvers
        .iter()
        .filter(|r| r.domain.as_deref().is_some_and(|d| host == d || host.ends_with(&format!(".{d}"))))
        .max_by_key(|r| r.domain.as_ref().map_or(0, String::len));
    scoped.or_else(|| resolvers.iter().find(|r| r.domain.is_none()))
}

// ---------------------------------------------------------------------------
// Answers
// ---------------------------------------------------------------------------

/// The record types asked for a host, all at once.
pub const KINDS: [&str; 7] = ["A", "AAAA", "CNAME", "MX", "NS", "TXT", "SOA"];

#[derive(Debug, Clone, PartialEq)]
pub struct Record {
    pub name: String,
    pub ttl: u32,
    pub kind: String,
    pub data: String,
}

/// One query's answer, as `dig` would show it.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Answer {
    /// `NOERROR`, `NXDOMAIN` and so on.
    pub status: String,
    pub answer: Vec<Record>,
    pub authority: Vec<Record>,
    pub query_ms: Option<u32>,
    pub server: Option<String>,
    /// Why there's no answer: the query failed, or timed out.
    pub error: Option<String>,
}

impl Answer {
    fn of_kind(&self, kind: &str) -> Vec<Record> {
        self.answer.iter().filter(|r| r.kind == kind).cloned().collect()
    }

    /// No answer, because of `error`.
    pub fn failed(error: impl Into<String>) -> Self {
        Answer { error: Some(error.into()), ..Answer::default() }
    }
}

/// The names in a reverse lookup's answer.
pub fn ptr_names(answer: &Answer) -> Vec<String> {
    answer.answer.iter().filter(|r| r.kind == "PTR").map(|r| r.data.clone()).collect()
}

// ---------------------------------------------------------------------------
// The report
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Level {
    Success,
    Warning,
    Error,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Notice {
    pub level: Level,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Row {
    pub key: String,
    pub value: String,
    pub hint: Option<String>,
}

fn row(key: impl Into<String>, value: impl Into<String>) -> Row {
    Row { key: key.into(), value: value.into(), hint: None }
}

impl Row {
    fn hint(mut self, hint: impl Into<String>) -> Self {
        self.hint = Some(hint.into());
        self
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Section {
    pub title: &'static str,
    pub rows: Vec<Row>,
}

/// What the lookup found, and what to copy.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct Report {
    pub notices: Vec<Notice>,
    pub sections: Vec<Section>,
    /// The addresses, one per line.
    pub addresses: Option<String>,
    /// The reverse lookup's names, one per line.
    pub names: Option<String>,
    pub dig_command: Option<String>,
}

impl Report {
    /// The lookup found something: addresses or names (not just a reason it found none).
    pub fn found(&self) -> bool {
        self.addresses.is_some() || self.names.is_some()
    }

    fn notice(&mut self, level: Level, text: impl Into<String>) {
        self.notices.push(Notice { level, text: text.into() });
    }

    fn section(&mut self, title: &'static str, rows: Vec<Row>) {
        if !rows.is_empty() {
            self.sections.push(Section { title, rows });
        }
    }
}

pub fn ttl(seconds: u32) -> String {
    match seconds {
        s if s >= 86_400 && s % 86_400 == 0 => format!("TTL {}d", s / 86_400),
        s if s >= 3600 && s % 3600 == 0 => format!("TTL {}h", s / 3600),
        s if s >= 60 && s % 60 == 0 => format!("TTL {}m", s / 60),
        s => format!("TTL {s}s"),
    }
}

fn rows(records: &[Record], key: impl Fn(&Record) -> String) -> Vec<Row> {
    records.iter().map(|r| row(key(r), r.data.clone()).hint(ttl(r.ttl))).collect()
}

fn resolver_rows(resolver: Option<&Resolver>, answered_by: Option<&str>) -> Vec<Row> {
    let mut rows = Vec::new();
    if let Some(server) = answered_by {
        let scope = resolver
            .and_then(|r| r.domain.clone())
            .map_or_else(|| "default resolver".to_string(), |d| format!("scoped to *.{d}"));
        rows.push(row("Answered by", server).hint(scope));
    }
    if let Some(r) = resolver.filter(|r| !r.search.is_empty()) {
        rows.push(row("Search domains", r.search.join(", ")));
    }
    rows
}

fn joined(ips: &[IpAddr]) -> String {
    ips.iter().map(ToString::to_string).collect::<Vec<_>>().join(", ")
}

/// Everything asked about a host.
pub struct HostAnswers {
    /// One per [`KINDS`], in order.
    pub answers: Vec<Answer>,
    /// What apps get (the system resolver), and how long it took.
    pub system: (Result<Vec<IpAddr>, String>, u128),
    /// Reverse lookups of the addresses found.
    pub reverse: Vec<(IpAddr, Answer)>,
}

impl HostAnswers {
    fn get(&self, kind: &str) -> &Answer {
        &self.answers[KINDS.iter().position(|k| *k == kind).expect("a known kind")]
    }
}

/// The addresses in the A and AAAA answers.
pub fn addresses(answers: &[Answer]) -> Vec<IpAddr> {
    let mut ips: Vec<IpAddr> = answers
        .iter()
        .take(2)
        .zip(["A", "AAAA"])
        .flat_map(|(answer, kind)| answer.of_kind(kind))
        .filter_map(|r| r.data.parse().ok())
        .collect();
    ips.sort();
    ips.dedup();
    ips
}

pub fn host_report(host: &str, resolver: Option<&Resolver>, found: &HostAnswers) -> Report {
    let server = resolver.and_then(|r| r.nameservers.first()).map(String::as_str);
    let (a, aaaa) = (found.get("A"), found.get("AAAA"));
    let dns_ips = addresses(&found.answers);
    // CNAMEs come back in the A answers as the chain to the address.
    let mut chain = a.of_kind("CNAME");
    if chain.is_empty() {
        chain = found.get("CNAME").of_kind("CNAME");
    }

    let mut report = Report::default();
    let server_label = a.server.as_deref().or(server).unwrap_or("?");
    let time = a.query_ms.map(|ms| format!(" · {ms} ms")).unwrap_or_default();
    match (&a.error, a.status.as_str(), dns_ips.is_empty()) {
        (Some(e), _, _) => report.notice(Level::Error, format!("DNS query failed via {server_label}: {e}")),
        (_, "NXDOMAIN", _) => {
            report.notice(Level::Error, format!("{host} does not exist (NXDOMAIN) — per {server_label}{time}"))
        }
        (_, "NOERROR", true) => report.notice(
            Level::Warning,
            format!("{host} exists but has no A/AAAA records — per {server_label}{time}"),
        ),
        (_, "NOERROR", false) => {
            report.notice(Level::Success, format!("{host} → {} — per {server_label}{time}", joined(&dns_ips)))
        }
        (_, other, _) => report.notice(Level::Error, format!("DNS status {other} from {server_label}{time}")),
    }

    // What apps get, if it differs.
    let (system, system_ms) = &found.system;
    match system {
        Ok(ips) if !ips.is_empty() && *ips != dns_ips => report.notice(
            Level::Warning,
            "The system resolver (what apps use) returns different addresses — check /etc/hosts, VPN or proxy settings",
        ),
        Err(e) if !dns_ips.is_empty() => {
            report.notice(Level::Warning, format!("DNS answers, but the system resolver fails: {e}"))
        }
        _ => {}
    }

    // Records. Behind a CNAME they belong to the target name: say so.
    let owner = |r: &Record| if r.name == host { r.kind.clone() } else { r.name.clone() };
    let records: Vec<Record> = a.of_kind("A").into_iter().chain(aaaa.of_kind("AAAA")).collect();
    report.section("Addresses", rows(&records, |r| r.kind.clone()));
    report.section(
        "CNAME chain",
        chain.iter().map(|r| row(r.name.clone(), format!("→ {}", r.data)).hint(ttl(r.ttl))).collect(),
    );
    report.section("Mail (MX)", rows(&found.get("MX").of_kind("MX"), owner));
    report.section("Name servers (NS)", rows(&found.get("NS").of_kind("NS"), owner));
    report.section("TXT", rows(&found.get("TXT").of_kind("TXT"), owner));
    // SOA: the host's own, or its zone's (from the authority section).
    let soa = found.get("SOA");
    if let Some(r) = soa.answer.iter().chain(&soa.authority).find(|r| r.kind == "SOA") {
        let fields: Vec<&str> = r.data.split_whitespace().collect();
        let mut soa_rows = vec![row("Zone", r.name.clone())];
        if fields.len() >= 3 {
            soa_rows.push(row("Primary NS", fields[0].trim_end_matches('.')));
            soa_rows.push(row("Admin", fields[1].trim_end_matches('.')));
            soa_rows.push(row("Serial", fields[2]));
        }
        report.section("Zone (SOA)", soa_rows);
    }

    let system_row = match system {
        Ok(ips) if ips.is_empty() => row("Addresses", "none"),
        Ok(ips) => row("Addresses", joined(ips)),
        Err(e) => row("Error", e.clone()),
    }
    .hint(format!("{system_ms} ms"));
    report.section("System resolver", vec![system_row]);
    let reverse = found
        .reverse
        .iter()
        .map(|(ip, answer)| {
            let names = ptr_names(answer);
            row(ip.to_string(), if names.is_empty() { "no PTR record".into() } else { names.join(", ") })
        })
        .collect();
    report.section("Reverse (PTR)", reverse);
    report.section("Resolver", resolver_rows(resolver, a.server.as_deref().or(server)));

    let ips: Vec<IpAddr> = if dns_ips.is_empty() { system.clone().unwrap_or_default() } else { dns_ips };
    report.addresses = (!ips.is_empty()).then(|| ips.iter().map(ToString::to_string).collect::<Vec<_>>().join("\n"));
    report.dig_command = Some(match server {
        Some(s) => format!("dig @{s} {host} A"),
        None => format!("dig {host} A"),
    });
    report
}

pub fn ip_report(ip: IpAddr, resolver: Option<&Resolver>, ptr: &Answer) -> Report {
    let server = resolver.and_then(|r| r.nameservers.first()).map(String::as_str);
    let names = ptr_names(ptr);
    let time = ptr.query_ms.map(|ms| format!(" · {ms} ms")).unwrap_or_default();
    let mut report = Report::default();
    match (&ptr.error, names.is_empty()) {
        (Some(e), _) => report.notice(Level::Error, format!("Reverse lookup failed: {e}")),
        (_, true) => report.notice(Level::Warning, format!("No PTR record for {ip}{time}")),
        (_, false) => report.notice(Level::Success, format!("{ip} → {}{time}", names.join(", "))),
    }
    let private = match ip {
        IpAddr::V4(v4) => v4.is_private() || v4.is_link_local(),
        IpAddr::V6(v6) => v6.is_unicast_link_local() || (v6.segments()[0] & 0xfe00) == 0xfc00,
    };
    let range = if ip.is_loopback() {
        "loopback"
    } else if private {
        "private"
    } else {
        "public"
    };
    report.section("Address", vec![row("Version", if ip.is_ipv4() { "IPv4" } else { "IPv6" }), row("Range", range)]);
    report.section("Resolver", resolver_rows(resolver, ptr.server.as_deref().or(server)));
    report.names = (!names.is_empty()).then(|| names.join("\n"));
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn finds_the_host() {
        let host = |s: &str| target(s).map(|(t, _)| t);
        assert_eq!(host("api.corp.example"), Some(Target::Host("api.corp.example".into())));
        assert_eq!(host("https://User@Api.Example.com:8443/v1?x=1"), Some(Target::Host("api.example.com".into())));
        assert_eq!(host("example.com:443"), Some(Target::Host("example.com".into())));
        assert_eq!(host("10.1.2.3"), Some(Target::Ip("10.1.2.3".parse().unwrap())));
        assert_eq!(host("http://[::1]:80/"), Some(Target::Ip("::1".parse().unwrap())));
        assert_eq!(host("hello world"), None);
        assert_eq!(host("{\"a\":1}"), None);
        assert_eq!(host("file.123"), None);
    }

    #[test]
    fn picks_the_scoped_resolver() {
        let resolver = |domain: Option<&str>, server: &str| Resolver {
            domain: domain.map(Into::into),
            nameservers: vec![server.into()],
            search: Vec::new(),
        };
        let rs = [resolver(None, "10.0.0.1"), resolver(Some("corp.example"), "172.16.0.2")];
        assert_eq!(resolver_for(&rs, "api.corp.example").unwrap().nameservers, ["172.16.0.2"]);
        assert_eq!(resolver_for(&rs, "corp.example").unwrap().nameservers, ["172.16.0.2"]);
        assert_eq!(resolver_for(&rs, "example.com").unwrap().nameservers, ["10.0.0.1"]);
    }

    #[test]
    fn formats_ttls() {
        assert_eq!(ttl(86_400), "TTL 1d");
        assert_eq!(ttl(300), "TTL 5m");
        assert_eq!(ttl(50), "TTL 50s");
    }

    #[test]
    fn reports_a_host_and_a_differing_system_resolver() {
        let a = Answer {
            status: "NOERROR".into(),
            answer: vec![Record { name: "x.com".into(), ttl: 60, kind: "A".into(), data: "1.2.3.4".into() }],
            query_ms: Some(9),
            server: Some("10.0.0.1".into()),
            ..Answer::default()
        };
        let mut answers = vec![Answer::default(); KINDS.len()];
        answers[0] = a;
        let found = HostAnswers { answers, system: (Ok(vec!["5.6.7.8".parse().unwrap()]), 3), reverse: Vec::new() };
        let resolver = Resolver { nameservers: vec!["10.0.0.1".into()], ..Resolver::default() };
        let report = host_report("x.com", Some(&resolver), &found);
        assert_eq!(report.notices[0], Notice { level: Level::Success, text: "x.com → 1.2.3.4 — per 10.0.0.1 · 9 ms".into() });
        assert_eq!(report.notices[1].level, Level::Warning, "the system resolver differs");
        assert_eq!(report.addresses.as_deref(), Some("1.2.3.4"));
        assert_eq!(report.dig_command.as_deref(), Some("dig @10.0.0.1 x.com A"));
        assert!(report.found());
    }

    #[test]
    fn a_lookup_that_found_nothing_is_not_found() {
        let found = HostAnswers { answers: vec![Answer::default(); KINDS.len()], system: (Ok(Vec::new()), 3), reverse: Vec::new() };
        let report = host_report("nothing.invalid", None, &found);
        assert!(!report.found(), "{report:?}");
    }
}
