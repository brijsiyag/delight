//! The network the app offers plugins with the `Network` permission ([`HttpApi`]):
//! HTTP/1.1 and HTTP/2 requests, streamed both ways (so gRPC works over them), and
//! HTTP listeners on `127.0.0.1` for callbacks such as a sign-in's redirect. All of
//! it runs natively, on one small tokio runtime, with TLS checked by macOS
//! (`rustls-platform-verifier`), so a company's TLS proxy works as it does in Safari.

mod exchange;
mod listener;

use std::cell::Cell;
use std::convert::Infallible;
use std::pin::Pin;
use std::task::Poll;
use std::net::Ipv4Addr;
use std::rc::Rc;
use std::sync::OnceLock;
use std::time::Duration;

use anyhow::{Context as _, Result, anyhow};
use delight_protocol::{HttpApi, HttpExchangeApi, HttpListening, HttpReceiverApi, HttpRequestHead, HttpResponderApi};
use embedded_gpui::gpui::{AppContext as _, Context, Task};
use embedded_gpui::{Ref, Registry, shared};
use http_body_util::StreamBody;
use hyper::body::Frame;
use hyper_rustls::HttpsConnector;
use hyper_util::client::legacy::Client;
use hyper_util::client::legacy::connect::HttpConnector;
use hyper_util::rt::TokioExecutor;

use exchange::Exchange;
use listener::Listener;

/// Most requests a plugin has open at once.
const MAX_EXCHANGES: usize = 17;
/// Most listeners a plugin has at once.
const MAX_LISTENERS: usize = 2;
/// Longest a plugin gets to take a piece of a response, or to answer a listener's
/// request, before the app gives up on it.
const PLUGIN_TIMEOUT: Duration = Duration::from_secs(30);

/// A request's body, as the plugin sends it: frames through a channel, and its length
/// when the plugin knew it (so hyper sends a `Content-Length`, or no body at all, not
/// chunks).
struct RequestBody {
    frames: StreamBody<futures::channel::mpsc::Receiver<Result<Frame<bytes::Bytes>, Infallible>>>,
    length: Option<u64>,
}

impl hyper::body::Body for RequestBody {
    type Data = bytes::Bytes;
    type Error = Infallible;

    fn poll_frame(
        mut self: Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> Poll<Option<Result<Frame<bytes::Bytes>, Infallible>>> {
        Pin::new(&mut self.frames).poll_frame(cx)
    }

    fn is_end_stream(&self) -> bool {
        self.length == Some(0)
    }

    fn size_hint(&self) -> hyper::body::SizeHint {
        self.length.map_or_else(hyper::body::SizeHint::default, hyper::body::SizeHint::with_exact)
    }
}

/// The HTTP object one plugin is given, and what it has open.
pub(crate) struct Http {
    registry: Registry,
    exchanges: Rc<Cell<usize>>,
    listeners: Rc<Cell<usize>>,
}

impl Http {
    pub(crate) fn new(registry: Registry) -> Self {
        Http { registry, exchanges: Rc::default(), listeners: Rc::default() }
    }
}

#[shared]
impl HttpApi for Http {
    fn start_exchange(
        &mut self,
        head: HttpRequestHead,
        receiver: Ref<HttpReceiverApi>,
        cx: &mut Context<Self>,
    ) -> Task<Result<Ref<HttpExchangeApi>>> {
        let receiver = receiver.connect();
        let open = Opened::count(&self.exchanges, MAX_EXCHANGES, "requests");
        let exchange = cx.new(|cx| Exchange::start(head, receiver, open, cx));
        Task::ready(Ok(self.registry.share(&exchange, cx)))
    }

    fn listen_http(
        &mut self,
        port: u16,
        responder: Ref<HttpResponderApi>,
        cx: &mut Context<Self>,
    ) -> Task<Result<HttpListening>> {
        let open = match Opened::count(&self.listeners, MAX_LISTENERS, "listeners") {
            Ok(open) => open,
            Err(error) => return Task::ready(Err(error)),
        };
        let responder = responder.connect();
        let registry = self.registry.clone();
        // Bound on the runtime, whose reactor the listener then belongs to.
        let bound = runtime().spawn(async move {
            let listener = tokio::net::TcpListener::bind((Ipv4Addr::LOCALHOST, port)).await?;
            let port = listener.local_addr()?.port();
            Ok::<_, std::io::Error>((listener, port))
        });
        cx.spawn(async move |_, cx| {
            let (listener, port) = bound.await.context("the network stopped")?.with_context(|| {
                format!("listening on 127.0.0.1:{port}")
            })?;
            let listener = cx.new(|cx| Listener::serve(listener, responder, open, cx));
            let listener = cx.update(|cx| registry.share(&listener, cx));
            Ok(HttpListening { port, listener })
        })
    }
}

/// One of a plugin's open requests or listeners, counted until it's dropped.
struct Opened(Rc<Cell<usize>>);

impl Opened {
    /// Count one more, unless the plugin already has `max` of them.
    fn count(open: &Rc<Cell<usize>>, max: usize, what: &str) -> Result<Opened> {
        if open.get() >= max {
            return Err(anyhow!("a plugin has at most {max} {what} open"));
        }
        open.set(open.get() + 1);
        Ok(Opened(open.clone()))
    }
}

impl Drop for Opened {
    fn drop(&mut self) {
        self.0.set(self.0.get() - 1);
    }
}

/// Aborts its task on the runtime when dropped: dropping a request cancels it.
struct AbortOnDrop(tokio::task::AbortHandle);

impl Drop for AbortOnDrop {
    fn drop(&mut self) {
        self.0.abort();
    }
}

/// The runtime the app's network runs on: two threads, started on first use.
fn runtime() -> &'static tokio::runtime::Runtime {
    static RUNTIME: OnceLock<tokio::runtime::Runtime> = OnceLock::new();
    RUNTIME.get_or_init(|| {
        tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .thread_name("delight-network")
            .enable_all()
            .build()
            .expect("starting the network's runtime")
    })
}

/// The HTTP clients every plugin shares (their connections are pooled): one that
/// speaks HTTP/1.1 or, where TLS negotiates it, HTTP/2; one that speaks HTTP/2 from
/// the start, for gRPC over `http://`.
struct Clients {
    any: Client<HttpsConnector<HttpConnector>, RequestBody>,
    http2: Client<HttpsConnector<HttpConnector>, RequestBody>,
}

fn clients() -> Result<&'static Clients> {
    static CLIENTS: OnceLock<Result<Clients, String>> = OnceLock::new();
    CLIENTS
        .get_or_init(|| {
            // One TLS backend: install it as the process's, so nothing guesses.
            let _ = rustls::crypto::ring::default_provider().install_default();
            let connector = hyper_rustls::HttpsConnectorBuilder::new()
                .with_platform_verifier()
                .https_or_http()
                .enable_all_versions()
                .build();
            let _guard = runtime().enter();
            Ok(Clients {
                any: Client::builder(TokioExecutor::new()).build(connector.clone()),
                http2: Client::builder(TokioExecutor::new()).http2_only(true).build(connector),
            })
        })
        .as_ref()
        .map_err(|error| anyhow!("{error}"))
}

/// An error and what caused it, as one line: hyper's own say little ("client error
/// (Connect)") and keep the reason in their sources.
fn describe(error: &(dyn std::error::Error + 'static)) -> String {
    let mut text = error.to_string();
    let mut source = error.source();
    while let Some(cause) = source {
        text.push_str(": ");
        text.push_str(&cause.to_string());
        source = cause.source();
    }
    text
}
