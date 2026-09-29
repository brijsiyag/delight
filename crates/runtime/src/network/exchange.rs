//! One request, from the plugin's head and body to its receiver.

use futures::channel::mpsc;
use futures::{SinkExt as _, StreamExt as _};
use http_body_util::{BodyExt as _, StreamBody};
use hyper::body::Frame;

use anyhow::{Result, anyhow};
use delight_protocol::{
    Bytes, HttpExchangeApi, HttpHeader, HttpReceiverApi, HttpReceiverApiCaller as _, HttpRequestHead,
    HttpResponseHead, headers_from_wire, headers_to_wire,
};
use embedded_gpui::gpui::{AppContext as _, AsyncApp, Context, Task};
use embedded_gpui::{Remote, shared};

use super::{AbortOnDrop, Opened, PLUGIN_TIMEOUT, RequestBody, clients, describe, runtime};

/// The most of a response's body sent to the plugin in one call: pieces cross as
/// base64 in a turn of the plugin, so they stay small.
const CHUNK: usize = 64 * 1024;

/// What the request's task tells the plugin's side, in order.
enum Event {
    Head(HttpResponseHead),
    Data(Vec<u8>),
    End(Vec<HttpHeader>),
    Failed(String),
}

pub(super) struct Exchange {
    /// Where the request's body goes, until the plugin finishes it.
    body: Option<mpsc::Sender<Result<Frame<bytes::Bytes>, std::convert::Infallible>>>,
    /// Handing the response to the plugin.
    _delivering: Task<()>,
    /// Sending the request: aborted when the plugin drops this.
    _request: Option<AbortOnDrop>,
    _open: Option<Opened>,
}

impl Exchange {
    /// Start sending the request `head` describes (its body follows), and hand the
    /// response to `receiver` as it comes. `open` counts it; an error there fails it.
    pub(super) fn start(
        head: HttpRequestHead,
        receiver: Remote<HttpReceiverApi>,
        open: Result<Opened>,
        cx: &mut Context<Self>,
    ) -> Self {
        let (body, body_frames) = mpsc::channel(4);
        let (events, mut delivered) = mpsc::channel(4);
        let started = open.and_then(|open| {
            let request = head.to_request(RequestBody { frames: StreamBody::new(body_frames), length: head.length })?;
            let task = runtime().spawn(send(request, events.clone()));
            Ok((open, AbortOnDrop(task.abort_handle())))
        });
        let (open, request) = match started {
            Ok((open, request)) => (Some(open), Some(request)),
            Err(error) => {
                let mut events = events;
                let _ = events.try_send(Event::Failed(format!("{error:#}")));
                (None, None)
            }
        };
        let delivering = cx.spawn(async move |_, cx| {
            let (mut trailers, mut error) = (Vec::new(), None);
            while let Some(event) = delivered.next().await {
                let taken = match event {
                    Event::Head(head) => within(cx, |cx| receiver.response_started(head, cx)).await,
                    Event::Data(data) => within(cx, |cx| receiver.response_data(data.into(), cx)).await,
                    Event::End(end) => {
                        trailers = end;
                        break;
                    }
                    Event::Failed(failure) => {
                        error = Some(failure);
                        break;
                    }
                };
                // A plugin that stopped taking the response: drop the request.
                if taken.is_err() {
                    return;
                }
            }
            drop(within(cx, |cx| receiver.response_ended(trailers, error, cx)).await);
        });
        Exchange { body: Some(body), _delivering: delivering, _request: request, _open: open }
    }
}

/// Make a call on the plugin, and wait for its answer for at most
/// [`PLUGIN_TIMEOUT`].
async fn within<F>(cx: &mut AsyncApp, call: impl FnOnce(&mut embedded_gpui::gpui::App) -> F) -> Result<()>
where
    F: Future<Output = Result<()>>,
{
    let answer = cx.update(call);
    let timer = cx.background_executor().timer(PLUGIN_TIMEOUT);
    match futures::future::select(std::pin::pin!(answer), timer).await {
        futures::future::Either::Left((answer, _)) => answer,
        futures::future::Either::Right(_) => Err(anyhow!("the plugin didn't take the response")),
    }
}

/// Send `request` and tell `events` what comes back, on the runtime.
async fn send(request: http::Request<RequestBody>, mut events: mpsc::Sender<Event>) {
    let clients = match clients() {
        Ok(clients) => clients,
        Err(error) => {
            let _ = events.send(Event::Failed(format!("{error:#}"))).await;
            return;
        }
    };
    let client = if request.version() == http::Version::HTTP_2 { &clients.http2 } else { &clients.any };
    let response = match client.request(request).await {
        Ok(response) => response,
        Err(error) => {
            let _ = events.send(Event::Failed(describe(&error))).await;
            return;
        }
    };
    let (parts, mut body) = response.into_parts();
    if events.send(Event::Head(HttpResponseHead::from_parts(&parts))).await.is_err() {
        return;
    }
    let mut trailers = Vec::new();
    while let Some(frame) = body.frame().await {
        let frame = match frame {
            Ok(frame) => frame,
            Err(error) => {
                let _ = events.send(Event::Failed(describe(&error))).await;
                return;
            }
        };
        match frame.into_data() {
            Ok(data) => {
                for chunk in data.chunks(CHUNK) {
                    if events.send(Event::Data(chunk.to_vec())).await.is_err() {
                        return;
                    }
                }
            }
            Err(frame) => {
                if let Ok(end) = frame.into_trailers() {
                    trailers.extend(headers_to_wire(&end));
                }
            }
        }
    }
    let _ = events.send(Event::End(trailers)).await;
}

#[shared]
impl HttpExchangeApi for Exchange {
    fn send_request_data(&mut self, data: Bytes, cx: &mut Context<Self>) -> Task<Result<()>> {
        let Some(mut body) = self.body.clone() else {
            return Task::ready(Err(anyhow!("the request's body is already finished")));
        };
        cx.background_spawn(async move {
            body.send(Ok(Frame::data(data.0.into()))).await.map_err(|_| anyhow!("the request is over"))
        })
    }

    fn finish_request(&mut self, trailers: Vec<HttpHeader>, cx: &mut Context<Self>) {
        let Some(mut body) = self.body.take() else { return };
        match headers_from_wire(&trailers) {
            Ok(trailers) if !trailers.is_empty() => {
                cx.background_spawn(async move { drop(body.send(Ok(Frame::trailers(trailers))).await) }).detach();
            }
            Ok(_) => {}
            Err(error) => log::warn!("a plugin's request trailers: {error:#}"),
        }
    }
}
