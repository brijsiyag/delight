//! A plugin's HTTP listener on `127.0.0.1`: each request is the plugin's to answer.

use std::convert::Infallible;

use anyhow::Result;
use delight_protocol::{HttpHit, HttpListenerApi, HttpReply, HttpRequestHead, HttpResponderApi, HttpResponderApiCaller as _};
use embedded_gpui::gpui::{Context, Task};
use embedded_gpui::{Remote, shared};
use futures::channel::{mpsc, oneshot};
use futures::{SinkExt as _, StreamExt as _};
use http_body_util::{BodyExt as _, Full, Limited};
use hyper::body::Incoming;
use hyper_util::rt::{TokioExecutor, TokioIo};

use super::{AbortOnDrop, Opened, PLUGIN_TIMEOUT, runtime};

/// The largest request body a listener takes (a callback's is small).
const MAX_BODY: usize = 1024 * 1024;

/// A request for the plugin, and where its answer goes.
type Hit = (HttpHit, oneshot::Sender<HttpReply>);

pub(super) struct Listener {
    /// Accepting connections, on the runtime: aborted when this is dropped or closed.
    server: Option<AbortOnDrop>,
    _answering: Task<()>,
    _open: Opened,
}

impl Listener {
    /// Serve `listener`, asking `responder` for each request's answer.
    pub(super) fn serve(
        listener: tokio::net::TcpListener,
        responder: Remote<HttpResponderApi>,
        open: Opened,
        cx: &mut Context<Self>,
    ) -> Self {
        let (hits, mut asked) = mpsc::channel::<Hit>(8);
        let server = runtime().spawn(accept(listener, hits));
        // One at a time, in the order they came.
        let answering = cx.spawn(async move |_, cx| {
            while let Some((hit, answer)) = asked.next().await {
                let reply = cx.update(|cx| responder.respond(hit, cx));
                let timer = cx.background_executor().timer(PLUGIN_TIMEOUT);
                if let futures::future::Either::Left((Ok(reply), _)) =
                    futures::future::select(std::pin::pin!(reply), timer).await
                {
                    let _ = answer.send(reply);
                }
                // Otherwise `answer` is dropped, and the request gets an error page.
            }
        });
        Listener { server: Some(AbortOnDrop(server.abort_handle())), _answering: answering, _open: open }
    }
}

#[shared]
impl HttpListenerApi for Listener {
    fn close_listener(&mut self, _cx: &mut Context<Self>) {
        self.server = None;
    }
}

/// Accept connections and serve each, HTTP/1.1 or HTTP/2.
async fn accept(listener: tokio::net::TcpListener, hits: mpsc::Sender<Hit>) {
    loop {
        let Ok((stream, _)) = listener.accept().await else { continue };
        let hits = hits.clone();
        tokio::spawn(async move {
            let service = hyper::service::service_fn(move |request| handle(request, hits.clone()));
            let builder = hyper_util::server::conn::auto::Builder::new(TokioExecutor::new());
            drop(builder.serve_connection(TokioIo::new(stream), service).await);
        });
    }
}

/// One request: hand it to the plugin, and send back its answer.
async fn handle(
    request: http::Request<Incoming>,
    mut hits: mpsc::Sender<Hit>,
) -> Result<http::Response<Full<bytes::Bytes>>, Infallible> {
    let (parts, body) = request.into_parts();
    let Ok(body) = Limited::new(body, MAX_BODY).collect().await else {
        return Ok(page(413, "The request is too large."));
    };
    let hit = HttpHit { head: HttpRequestHead::from_parts(&parts), body: body.to_bytes().to_vec().into() };
    let (answer, answered) = oneshot::channel();
    if hits.send((hit, answer)).await.is_err() {
        return Ok(page(503, "The plugin isn't listening any more."));
    }
    Ok(match answered.await {
        Ok(reply) => reply
            .head
            .to_response(Full::new(reply.body.0.into()))
            .unwrap_or_else(|error| page(500, &format!("The plugin's answer isn't valid HTTP: {error:#}"))),
        Err(_) => page(504, "The plugin didn't answer."),
    })
}

/// A plain page from Delight itself, when the plugin's answer can't be had.
fn page(status: u16, text: &str) -> http::Response<Full<bytes::Bytes>> {
    let mut response = http::Response::new(Full::new(bytes::Bytes::from(text.to_string())));
    *response.status_mut() = http::StatusCode::from_u16(status).unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR);
    response.headers_mut().insert(http::header::CONTENT_TYPE, http::HeaderValue::from_static("text/plain; charset=utf-8"));
    response
}
