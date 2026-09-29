//! gRPC with tonic's own generated clients, over the app's HTTP/2:
//!
//! ```ignore
//! let channel = delight_plugin_api::network::grpc::channel("https://api.example.com".parse()?, cx);
//! let mut client = GreeterClient::new(channel);
//! let reply = client.say_hello(HelloRequest { name: "Delight".into() }).await?;
//! ```
//!
//! Unary, client-streaming, server-streaming and two-way calls all work: bodies
//! stream both ways, and gRPC's status arrives in the response's trailers. The
//! plugin's crate depends on `tonic` (the same version as the plugin API) and
//! generates its clients with `tonic-prost-build`, as any tonic project does. Needs
//! the `Network` permission.

use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll};

use crate::gpui::{App, AsyncApp};

type BoxError = Box<dyn std::error::Error + Send + Sync>;

/// A channel for tonic's clients: each call is a request through the app.
#[derive(Clone)]
pub struct Channel {
    /// The server: its scheme and authority (a call's path is added to it).
    base: http::Uri,
    cx: AsyncApp,
}

/// A channel to `base`, such as `https://api.example.com`.
pub fn channel(base: http::Uri, cx: &App) -> Channel {
    Channel { base, cx: cx.to_async() }
}

impl tower_service::Service<http::Request<tonic::body::Body>> for Channel {
    type Response = http::Response<tonic::body::Body>;
    type Error = BoxError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Response, BoxError>>>>;

    fn poll_ready(&mut self, _: &mut Context<'_>) -> Poll<Result<(), BoxError>> {
        Poll::Ready(Ok(()))
    }

    fn call(&mut self, request: http::Request<tonic::body::Body>) -> Self::Future {
        let (base, cx) = (self.base.clone(), self.cx.clone());
        Box::pin(imp::call(base, cx, request))
    }
}

#[cfg(target_arch = "wasm32")]
mod imp {
    use std::pin::Pin;
    use std::task::{Context, Poll, ready};

    use delight_protocol::{HttpExchangeApiCaller as _, HttpRequestHead, headers_from_wire, headers_to_wire};
    use futures::channel::{mpsc, oneshot};
    use futures::StreamExt as _;
    use http_body::Frame;
    use http_body_util::BodyExt as _;

    use super::BoxError;
    use crate::gpui::AsyncApp;
    use crate::host;
    use crate::network::imp::{Event, Started, http_api, start};

    pub(super) async fn call(
        base: http::Uri,
        mut cx: AsyncApp,
        request: http::Request<tonic::body::Body>,
    ) -> Result<http::Response<tonic::body::Body>, BoxError> {
        let (mut parts, mut body) = request.into_parts();
        parts.uri = joined(&base, &parts.uri)?;
        let remote = cx.update(|cx| host(cx).remote).ok_or("gRPC is Delight's: a plugin reaches it only in Delight")?;
        let api = http_api(&remote, &mut cx).await?;
        let Started { exchange, mut events } = start(&api, HttpRequestHead::from_parts(&parts), &mut cx).await?;
        // The request's body streams in the background; the exchange lives until the
        // response's body is done with (or dropped), since dropping it cancels.
        let (done, response_done) = oneshot::channel::<()>();
        cx.spawn(async move |cx| {
            let mut trailers = Vec::new();
            while let Some(Ok(frame)) = body.frame().await {
                match frame.into_data() {
                    Ok(data) => {
                        let sent = cx.update(|cx| exchange.send_request_data(data.to_vec().into(), cx));
                        if sent.await.is_err() {
                            break;
                        }
                    }
                    Err(frame) => {
                        if let Ok(end) = frame.into_trailers() {
                            trailers = headers_to_wire(&end);
                        }
                    }
                }
            }
            cx.update(|cx| drop(exchange.finish_request(trailers, cx)));
            let _ = response_done.await;
            drop(exchange);
        })
        .detach();
        match events.next().await {
            Some(Event::Head(head)) => {
                let body = ResponseBody { events, done: Some(done), ended: false };
                Ok(head.to_response(tonic::body::Body::new(body))?)
            }
            Some(Event::End(_, Some(error))) => Err(error.into()),
            _ => Err("the response ended before it began".into()),
        }
    }

    /// The call's URL: the channel's scheme and authority, with the call's path.
    fn joined(base: &http::Uri, call: &http::Uri) -> Result<http::Uri, BoxError> {
        let path = call.path_and_query().map_or("/", |path| path.as_str());
        let mut parts = base.clone().into_parts();
        parts.path_and_query = Some(path.parse()?);
        Ok(http::Uri::from_parts(parts)?)
    }

    /// The response's body as it arrives: data, then its trailers.
    struct ResponseBody {
        events: mpsc::Receiver<Event>,
        /// Dropped with the body, which lets the exchange go.
        done: Option<oneshot::Sender<()>>,
        ended: bool,
    }

    impl http_body::Body for ResponseBody {
        type Data = bytes::Bytes;
        type Error = tonic::Status;

        fn poll_frame(
            mut self: Pin<&mut Self>,
            cx: &mut Context<'_>,
        ) -> Poll<Option<Result<Frame<bytes::Bytes>, tonic::Status>>> {
            if self.ended {
                return Poll::Ready(None);
            }
            let frame = match ready!(self.events.poll_next_unpin(cx)) {
                Some(Event::Data(data)) => Some(Ok(Frame::data(data.into()))),
                Some(Event::End(trailers, error)) => {
                    self.ended = true;
                    self.done = None;
                    match (error, headers_from_wire(&trailers)) {
                        (Some(error), _) => Some(Err(tonic::Status::unavailable(error))),
                        (None, Ok(trailers)) if trailers.is_empty() => None,
                        (None, Ok(trailers)) => Some(Ok(Frame::trailers(trailers))),
                        (None, Err(error)) => Some(Err(tonic::Status::internal(format!("{error:#}")))),
                    }
                }
                Some(Event::Head(_)) | None => {
                    self.ended = true;
                    None
                }
            };
            Poll::Ready(frame)
        }
    }
}

/// Natively (a plugin's unit tests) there's no app to call.
#[cfg(not(target_arch = "wasm32"))]
mod imp {
    use super::BoxError;
    use crate::gpui::AsyncApp;

    pub(super) async fn call(
        _: http::Uri,
        _: AsyncApp,
        _: http::Request<tonic::body::Body>,
    ) -> Result<http::Response<tonic::body::Body>, BoxError> {
        Err("gRPC is Delight's: a plugin reaches it only in Delight".into())
    }
}
