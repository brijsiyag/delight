//! Asking: the Mac's resolvers from the app; DNS queries sent to them over UDP, as
//! `dig` does (every record type at once, then the reverse lookups); and the system
//! resolver, what apps get, through WASI's name lookup. A plugin has no threads, so
//! nothing here waits in place: sockets are non-blocking, and checked again on a
//! short timer until they answer.

use std::net::{IpAddr, Ipv4Addr, Ipv6Addr, SocketAddr, UdpSocket};
use std::time::{Duration, Instant};

use delight_plugin_api::host;
use futures::future::join_all;
use gpui::AsyncApp;
use hickory_proto::op::{Message, MessageType, OpCode, Query, ResponseCode};
use hickory_proto::rr::{Name, RecordType};

use crate::lookup::{self, Answer, HostAnswers, KINDS, Record, Report, Target};

/// At most this many addresses are looked up in reverse.
const REVERSE_LOOKUPS: usize = 4;
/// How long a DNS server gets to answer (`dig +time=2`).
const QUERY_TIMEOUT: Duration = Duration::from_secs(2);
/// How long the system resolver gets.
const SYSTEM_TIMEOUT: Duration = Duration::from_secs(5);
/// How often a waiting socket or lookup is checked.
const POLL: Duration = Duration::from_millis(10);

pub async fn lookup(target: Target, cx: &mut AsyncApp) -> Report {
    let resolvers = cx.update(|cx| host(cx).dns_resolvers(cx)).await.unwrap_or_default();
    match target {
        Target::Host(name) => {
            let resolver = lookup::resolver_for(&resolvers, &name);
            let server = resolver.and_then(|resolver| first_address(&resolver.nameservers));
            let queries = KINDS.iter().map(|kind| ask(server, name.clone(), kind, cx.clone()));
            let started = Instant::now();
            let (answers, system) = futures::join!(join_all(queries), system_lookup(&name, cx.clone()));
            let system = (system, started.elapsed().as_millis());
            let ips: Vec<IpAddr> = lookup::addresses(&answers).into_iter().take(REVERSE_LOOKUPS).collect();
            let reverse = join_all(ips.iter().map(|ip| ask(server, ip.to_string(), "PTR", cx.clone()))).await;
            let reverse = ips.into_iter().zip(reverse).collect();
            lookup::host_report(&name, resolver, &HostAnswers { answers, system, reverse })
        }
        Target::Ip(ip) => {
            let resolver = lookup::resolver_for(&resolvers, "");
            let server = resolver.and_then(|resolver| first_address(&resolver.nameservers));
            let ptr = ask(server, ip.to_string(), "PTR", cx.clone()).await;
            lookup::ip_report(ip, resolver, &ptr)
        }
    }
}

/// The first server that's a plain address (not `fe80::1%en0`, which can't be asked
/// from the sandbox).
fn first_address(nameservers: &[String]) -> Option<IpAddr> {
    nameservers.iter().find_map(|server| server.parse().ok())
}

/// Ask `server` for `name`'s `kind` records (`PTR`: `name` is an address to look up
/// in reverse).
async fn ask(server: Option<IpAddr>, name: String, kind: &str, cx: AsyncApp) -> Answer {
    let Some(server) = server else {
        return Answer::failed("no DNS server to ask");
    };
    let answer = match exchange(server, &name, kind, &cx).await {
        Ok(answer) => answer,
        Err(error) => Answer::failed(error),
    };
    Answer { server: Some(server.to_string()), ..answer }
}

async fn exchange(server: IpAddr, name: &str, kind: &str, cx: &AsyncApp) -> Result<Answer, String> {
    let (question, record_type) = if kind == "PTR" {
        let ip: IpAddr = name.parse().map_err(|_| format!("{name} isn't an address"))?;
        (Name::from(ip), RecordType::PTR)
    } else {
        let record_type = kind.parse::<RecordType>().map_err(|error| error.to_string())?;
        (Name::from_ascii(name).map_err(|error| error.to_string())?, record_type)
    };
    let id = query_id();
    let mut message = Message::new(id, MessageType::Query, OpCode::Query);
    message.metadata.recursion_desired = true;
    message.add_query(Query::query(question, record_type));
    let query = message.to_vec().map_err(|error| error.to_string())?;

    let local: SocketAddr = match server {
        IpAddr::V4(_) => (Ipv4Addr::UNSPECIFIED, 0).into(),
        IpAddr::V6(_) => (Ipv6Addr::UNSPECIFIED, 0).into(),
    };
    let socket = UdpSocket::bind(local).map_err(|error| format!("opening a socket: {error}"))?;
    socket.set_nonblocking(true).map_err(|error| error.to_string())?;
    socket.connect((server, 53)).map_err(|error| format!("{server}: {error}"))?;
    let started = Instant::now();
    socket.send(&query).map_err(|error| format!("asking {server}: {error}"))?;
    let mut buffer = vec![0; 65_535];
    loop {
        match socket.recv(&mut buffer) {
            Ok(length) => {
                let Ok(response) = Message::from_vec(&buffer[..length]) else { continue };
                // Someone else's answer (or a stale one): keep waiting for ours.
                if response.metadata.id != id {
                    continue;
                }
                return Ok(answer(&response, started.elapsed()));
            }
            Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                if started.elapsed() > QUERY_TIMEOUT {
                    return Err(format!("{server} didn't answer in {} s", QUERY_TIMEOUT.as_secs()));
                }
                cx.background_executor().timer(POLL).await;
            }
            Err(error) => return Err(format!("{server}: {error}")),
        }
    }
}

/// A query id that differs between queries (they run side by side).
fn query_id() -> u16 {
    use std::sync::atomic::{AtomicU16, Ordering};
    static NEXT: AtomicU16 = AtomicU16::new(0);
    let nanos = std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).map_or(0, |time| time.subsec_nanos());
    (nanos as u16) ^ NEXT.fetch_add(1, Ordering::Relaxed).wrapping_mul(40_503)
}

/// A response, as `dig` shows it: its status, and its records without trailing dots.
fn answer(response: &Message, took: Duration) -> Answer {
    let record = |record: &hickory_proto::rr::Record| Record {
        name: record.name.to_string().trim_end_matches('.').to_string(),
        ttl: record.ttl,
        kind: record.record_type().to_string(),
        data: record.data.to_string().trim_end_matches('.').to_string(),
    };
    let mut answer = Answer {
        status: status(response.metadata.response_code),
        answer: response.answers.iter().map(record).collect(),
        authority: response.authorities.iter().map(record).collect(),
        query_ms: Some(took.as_millis() as u32),
        ..Answer::default()
    };
    if response.metadata.truncation && answer.answer.is_empty() {
        answer.error = Some("the answer was too large for UDP".into());
    }
    answer
}

/// A response code as `dig` names it.
fn status(code: ResponseCode) -> String {
    match code {
        ResponseCode::NoError => "NOERROR".into(),
        ResponseCode::FormErr => "FORMERR".into(),
        ResponseCode::ServFail => "SERVFAIL".into(),
        ResponseCode::NXDomain => "NXDOMAIN".into(),
        ResponseCode::NotImp => "NOTIMP".into(),
        ResponseCode::Refused => "REFUSED".into(),
        other => format!("{other:?}").to_uppercase(),
    }
}

/// The addresses the system resolver gives `name` (`/etc/hosts`, a VPN's resolvers
/// and all): WASI's name lookup, answered by the Mac's.
#[cfg(target_arch = "wasm32")]
async fn system_lookup(name: &str, cx: AsyncApp) -> Result<Vec<IpAddr>, String> {
    use wasip2::sockets::instance_network::instance_network;
    use wasip2::sockets::ip_name_lookup::{ErrorCode, resolve_addresses};
    use wasip2::sockets::network::IpAddress;

    let stream = resolve_addresses(&instance_network(), name).map_err(|error| format!("{error:?}"))?;
    let started = Instant::now();
    let mut ips = Vec::new();
    loop {
        match stream.resolve_next_address() {
            Ok(Some(IpAddress::Ipv4((a, b, c, d)))) => ips.push(IpAddr::from([a, b, c, d])),
            Ok(Some(IpAddress::Ipv6((a, b, c, d, e, f, g, h)))) => ips.push(IpAddr::from([a, b, c, d, e, f, g, h])),
            Ok(None) => break,
            Err(ErrorCode::WouldBlock) => {
                if started.elapsed() > SYSTEM_TIMEOUT {
                    return Err("the system resolver didn't answer".into());
                }
                cx.background_executor().timer(POLL).await;
            }
            Err(ErrorCode::NameUnresolvable) => return Err("not found".into()),
            Err(error) => return Err(format!("{error:?}")),
        }
    }
    ips.sort();
    ips.dedup();
    Ok(ips)
}

/// Natively (the unit tests) there's no WASI.
#[cfg(not(target_arch = "wasm32"))]
async fn system_lookup(_: &str, _: AsyncApp) -> Result<Vec<IpAddr>, String> {
    let _ = SYSTEM_TIMEOUT;
    Err("no system lookup outside Delight".into())
}

#[cfg(test)]
mod tests {
    use super::*;
    use hickory_proto::rr::{RData, rdata};

    #[test]
    fn a_response_reads_as_dig_shows_it() {
        let mut response = Message::new(7, MessageType::Response, OpCode::Query);
        let name = Name::from_ascii("x.com.").unwrap();
        response.add_answer(hickory_proto::rr::Record::from_rdata(name, 60, RData::A(rdata::A::new(1, 2, 3, 4))));
        let answer = answer(&response, Duration::from_millis(9));
        assert_eq!(answer.status, "NOERROR");
        assert_eq!(answer.answer, [Record { name: "x.com".into(), ttl: 60, kind: "A".into(), data: "1.2.3.4".into() }]);
        assert_eq!(answer.query_ms, Some(9));
        assert_eq!(status(ResponseCode::NXDomain), "NXDOMAIN");
        assert_eq!(first_address(&["fe80::1%en0".into(), "10.0.0.1".into()]), Some("10.0.0.1".parse().unwrap()));
    }
}
