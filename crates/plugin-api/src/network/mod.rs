//! The network, done by the app for plugins with the `Network` permission: HTTP with
//! the `http` crate's own types ([`Host::http`]), HTTP callbacks on this Mac
//! ([`Host::listen_http`]), and gRPC with tonic's generated clients (`grpc`, with the
//! `grpc` feature). The app speaks HTTP/1.1 and HTTP/2, with TLS checked by macOS; a
//! plugin never blocks on it. A `Network` plugin also has WASI's sockets.

use anyhow::{Result, anyhow};

use crate::Host;
use crate::gpui::{App, Task};

/// A request with its whole body: `http::Request::get(url).body(Vec::new())?`.
pub type Request = http::Request<Vec<u8>>;
/// A response with its whole body.
pub type Response = http::Response<Vec<u8>>;

/// Answers a listener's requests: the request, and what to send back.
pub type Respond = dyn Fn(Request, &mut App) -> Task<Response>;

/// A listener on `127.0.0.1` ([`Host::listen_http`]). It listens until it's dropped.
pub struct HttpListener {
    port: u16,
    /// Holding it keeps the app's listener open.
    _listener: Option<embedded_gpui::Remote<delight_protocol::HttpListenerApi>>,
}

impl HttpListener {
    /// The port it listens on, on `127.0.0.1`: for a redirect URL such as
    /// `http://127.0.0.1:{port}/callback`.
    pub fn port(&self) -> u16 {
        self.port
    }
}

impl Host {
    /// Send `request` and wait for its whole response, over HTTP/1.1 or HTTP/2 (set
    /// the request's version to HTTP/2 for a server that only speaks that over
    /// `http://`). Needs the `Network` permission; without it, and natively, an
    /// error. Dropping the task cancels the request.
    pub fn http(&self, request: Request, cx: &mut App) -> Task<Result<Response>> {
        match &self.remote {
            Some(remote) => imp::http(remote.clone(), request, cx),
            None => Task::ready(Err(outside())),
        }
    }

    /// Listen for HTTP on `127.0.0.1:port` (0: any free port), answering every request
    /// with `respond`: a sign-in's redirect back from the browser, for one. Nothing
    /// outside this Mac reaches it. Needs the `Network` permission.
    pub fn listen_http(
        &self,
        port: u16,
        respond: impl Fn(Request, &mut App) -> Task<Response> + 'static,
        cx: &mut App,
    ) -> Task<Result<HttpListener>> {
        match &self.remote {
            Some(remote) => imp::listen_http(remote.clone(), port, Box::new(respond), cx),
            None => Task::ready(Err(outside())),
        }
    }
}

/// Why there's no network: the plugin isn't running in Delight.
fn outside() -> anyhow::Error {
    anyhow!("the network is Delight's: a plugin reaches it only in Delight")
}

#[cfg_attr(not(target_arch = "wasm32"), allow(dead_code))]
const NO_NETWORK: &str = "this plugin doesn't have the Network permission";

#[cfg(target_arch = "wasm32")]
pub(crate) mod imp {
    use anyhow::{Context as _, Result, anyhow};
    use delight_protocol::{
        HostApi, HostApiCaller as _, HttpApi, HttpApiCaller as _, HttpExchangeApi, HttpExchangeApiCaller as _,
        HttpHeader, HttpHit, HttpReceiverApi, HttpReply, HttpRequestHead, HttpResponderApi, HttpResponseHead,
    };
    use embedded_gpui::{Remote, share, shared};
    use futures::channel::mpsc;
    use futures::{SinkExt as _, StreamExt as _};

    use super::{HttpListener, NO_NETWORK, Request, Respond, Response};
    use crate::gpui::{App, AppContext as _, AsyncApp, Context, Task};

    /// The most of a request's body sent in one call.
    const CHUNK: usize = 64 * 1024;

    /// The app's HTTP object for this plugin.
    pub(crate) async fn http_api(host: &Remote<HostApi>, cx: &mut AsyncApp) -> Result<Remote<HttpApi>> {
        let asked = cx.update(|cx| host.http(cx));
        Ok(asked.await?.context(NO_NETWORK)?.connect())
    }

    /// What arrives of a response, in order.
    pub(crate) enum Event {
        Head(HttpResponseHead),
        Data(Vec<u8>),
        /// Its trailers (read by gRPC), or why it failed.
        End(#[cfg_attr(not(feature = "grpc"), allow(dead_code))] Vec<HttpHeader>, Option<String>),
    }

    /// Where the app sends a response: into a small channel, whose room is the
    /// back-pressure (the app waits for each piece to be taken).
    struct Receiver {
        events: mpsc::Sender<Event>,
    }

    impl Receiver {
        fn pass(&self, event: Event, cx: &mut Context<Self>) -> Task<Result<()>> {
            let mut events = self.events.clone();
            cx.spawn(async move |_, _| events.send(event).await.map_err(|_| anyhow!("the response isn't wanted any more")))
        }
    }

    #[shared]
    impl HttpReceiverApi for Receiver {
        fn response_started(&mut self, head: HttpResponseHead, cx: &mut Context<Self>) -> Task<Result<()>> {
            self.pass(Event::Head(head), cx)
        }

        fn response_data(&mut self, data: delight_protocol::Bytes, cx: &mut Context<Self>) -> Task<Result<()>> {
            self.pass(Event::Data(data.0), cx)
        }

        fn response_ended(
            &mut self,
            trailers: Vec<HttpHeader>,
            error: Option<String>,
            cx: &mut Context<Self>,
        ) -> Task<Result<()>> {
            self.pass(Event::End(trailers, error), cx)
        }
    }

    /// A request on its way: where its body goes, and what comes back.
    pub(crate) struct Started {
        pub exchange: Remote<HttpExchangeApi>,
        pub events: mpsc::Receiver<Event>,
    }

    /// Start a request with `head`; its body goes through `exchange`, and the request
    /// is cancelled when the last clone of `exchange` is dropped.
    pub(crate) async fn start(api: &Remote<HttpApi>, head: HttpRequestHead, cx: &mut AsyncApp) -> Result<Started> {
        let (events, received) = mpsc::channel(4);
        let started = cx.update(|cx| {
            let receiver = cx.new(|_| Receiver { events });
            api.start_exchange(head, share(&receiver, cx), cx)
        });
        Ok(Started { exchange: started.await?, events: received })
    }

    /// Send `body` through `exchange`, a piece at a time, then finish the request.
    pub(crate) async fn send_body(exchange: &Remote<HttpExchangeApi>, body: &[u8], cx: &mut AsyncApp) -> Result<()> {
        for chunk in body.chunks(CHUNK) {
            let sent = cx.update(|cx| exchange.send_request_data(chunk.into(), cx));
            sent.await?;
        }
        cx.update(|cx| drop(exchange.finish_request(Vec::new(), cx)));
        Ok(())
    }

    pub(crate) fn http(host: Remote<HostApi>, request: Request, cx: &mut App) -> Task<Result<Response>> {
        cx.spawn(async move |cx| {
            let api = http_api(&host, cx).await?;
            let (parts, body) = request.into_parts();
            let head = HttpRequestHead { length: Some(body.len() as u64), ..HttpRequestHead::from_parts(&parts) };
            let Started { exchange, mut events } = start(&api, head, cx).await?;
            send_body(&exchange, &body, cx).await?;
            let (mut head, mut body) = (None, Vec::new());
            while let Some(event) = events.next().await {
                match event {
                    Event::Head(started) => head = Some(started),
                    Event::Data(data) => body.extend(data),
                    Event::End(_, Some(error)) => return Err(anyhow!("{error}")),
                    Event::End(_, None) => break,
                }
            }
            // The exchange lived until here: dropping it earlier would have cancelled it.
            drop(exchange);
            head.context("the response ended before it began")?.to_response(body)
        })
    }

    /// Answers a listener's requests with the plugin's `respond`.
    struct Responder {
        respond: Box<Respond>,
    }

    #[shared]
    impl HttpResponderApi for Responder {
        fn respond(&mut self, hit: HttpHit, cx: &mut Context<Self>) -> Task<Result<HttpReply>> {
            let request = match hit.head.to_request(hit.body.0) {
                Ok(request) => request,
                Err(error) => return Task::ready(Err(error)),
            };
            let answer = (self.respond)(request, cx);
            cx.spawn(async move |_, _| Ok(HttpReply::from(answer.await)))
        }
    }

    pub(crate) fn listen_http(
        host: Remote<HostApi>,
        port: u16,
        respond: Box<Respond>,
        cx: &mut App,
    ) -> Task<Result<HttpListener>> {
        let responder = cx.new(|_| Responder { respond });
        let responder = share(&responder, cx);
        cx.spawn(async move |cx| {
            let api = http_api(&host, cx).await?;
            let listening = cx.update(|cx| api.listen_http(port, responder, cx)).await?;
            Ok(HttpListener { port: listening.port, _listener: Some(listening.listener.connect()) })
        })
    }
}

/// Natively (a plugin's unit tests) there's no app to ask.
#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use anyhow::Result;
    use delight_protocol::HostApi;
    use embedded_gpui::Remote;

    use super::{HttpListener, Request, Respond, Response, outside};
    use crate::gpui::{App, Task};

    pub(crate) fn http(_: Remote<HostApi>, _: Request, _: &mut App) -> Task<Result<Response>> {
        Task::ready(Err(outside()))
    }

    pub(crate) fn listen_http(_: Remote<HostApi>, _: u16, _: Box<Respond>, _: &mut App) -> Task<Result<HttpListener>> {
        Task::ready(Err(outside()))
    }
}

#[cfg(feature = "grpc")]
pub mod grpc;
