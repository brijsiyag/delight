//! The network, as the app offers it to plugins with the `Network` permission: HTTP
//! (and so gRPC) through [`HttpApi::start_exchange`], and HTTP callbacks from this Mac
//! through [`HttpApi::listen_http`]. The app speaks HTTP/1.1 and HTTP/2 natively, with
//! TLS checked by macOS, so plugins never block on the network.
//!
//! The wire carries the `http` crate's requests and responses as the plain data here;
//! the conversions at the bottom are what both ends use.

use anyhow::{Context as _, Result};
use embedded_gpui::{Ref, data, interface};

use crate::Bytes;

/// HTTP for one plugin, homed in the app.
#[interface]
pub trait HttpApi {
    /// Send a request: its head now, its body through the returned exchange (then
    /// [`HttpExchangeApi::finish_request`]). The app calls `receiver` with the
    /// response as it arrives. Dropping the exchange cancels the request.
    ///
    /// Answered, not pipelined (a bare `Ref` return would be): a pipelined ref leaves the app
    /// holding the exchange under its real id after the plugin drops it, so the request
    /// would count as open for as long as the plugin lives.
    async fn start_exchange(
        &mut self,
        head: HttpRequestHead,
        receiver: Ref<HttpReceiverApi>,
        cx: &mut gpui::Context<Self>,
    ) -> Ref<HttpExchangeApi>;

    /// Listen for HTTP on `127.0.0.1:port` (0: any free port): every request that
    /// arrives is answered by `responder`. It stops when the plugin drops the listener.
    async fn listen_http(
        &mut self,
        port: u16,
        responder: Ref<HttpResponderApi>,
        cx: &mut gpui::Context<Self>,
    ) -> HttpListening;
}

/// One request on its way, homed in the app: the plugin sends its body through it.
#[interface]
pub trait HttpExchangeApi {
    /// More of the request's body. Answered once the app has room for more, so a
    /// plugin sending a large body waits instead of piling it up.
    async fn send_request_data(&mut self, data: Bytes, cx: &mut gpui::Context<Self>);

    /// The request's body is complete, ended by `trailers` (usually none).
    fn finish_request(&mut self, trailers: Vec<HttpHeader>, cx: &mut gpui::Context<Self>);
}

/// Where a response arrives, homed in the plugin. The app calls these in order, each
/// once the last is answered: the head, the body in pieces, then the end.
#[interface]
pub trait HttpReceiverApi {
    async fn response_started(&mut self, head: HttpResponseHead, cx: &mut gpui::Context<Self>);

    /// More of the response's body; answered when the plugin is ready for more.
    async fn response_data(&mut self, data: Bytes, cx: &mut gpui::Context<Self>);

    /// The response is complete, with its trailers (gRPC's status is there), or it
    /// failed with `error` (and then may have had no head).
    async fn response_ended(
        &mut self,
        trailers: Vec<HttpHeader>,
        error: Option<String>,
        cx: &mut gpui::Context<Self>,
    );
}

/// What answers the requests a listener gets, homed in the plugin.
#[interface]
pub trait HttpResponderApi {
    /// The response to `hit`, which the app sends back to whoever asked (such as the
    /// browser, after signing in). One the plugin doesn't answer in time gets an
    /// error page instead.
    async fn respond(&mut self, hit: HttpHit, cx: &mut gpui::Context<Self>) -> HttpReply;
}

/// A listener, homed in the app. It listens until the plugin drops it or closes it.
#[interface]
pub trait HttpListenerApi {
    fn close_listener(&mut self, cx: &mut gpui::Context<Self>);
}

/// A listener, and the port it got.
#[data]
pub struct HttpListening {
    pub port: u16,
    pub listener: Ref<HttpListenerApi>,
}

/// An HTTP header: its name, and its value's bytes (a value needn't be text).
#[data]
#[derive(PartialEq)]
pub struct HttpHeader {
    pub name: String,
    pub value: Bytes,
}

/// A request without its body.
#[data]
#[derive(PartialEq)]
pub struct HttpRequestHead {
    pub method: String,
    /// The full URL (`https://api.example.com/v1/items?page=2`); for a listener's
    /// requests, the path and query as they arrived.
    pub uri: String,
    /// HTTP/2 only: for `http://` without TLS, spoken from the start (gRPC); over
    /// TLS, HTTP/2 is used whenever the server offers it anyway.
    pub http2: bool,
    pub headers: Vec<HttpHeader>,
    /// The body's length, when it's known up front (a whole body), so the request is
    /// sent with its `Content-Length` (or without a body); `None` for a body that
    /// streams (gRPC).
    pub length: Option<u64>,
}

/// A response without its body.
#[data]
#[derive(PartialEq)]
pub struct HttpResponseHead {
    pub status: u16,
    pub headers: Vec<HttpHeader>,
}

/// A whole request, as a listener hands it to its plugin.
#[data]
#[derive(PartialEq)]
pub struct HttpHit {
    pub head: HttpRequestHead,
    pub body: Bytes,
}

/// A whole response, as a plugin answers a hit.
#[data]
#[derive(PartialEq)]
pub struct HttpReply {
    pub head: HttpResponseHead,
    pub body: Bytes,
}

// The `http` crate's types, to and from the wire.

/// `headers` as they cross.
pub fn headers_to_wire(headers: &http::HeaderMap) -> Vec<HttpHeader> {
    headers
        .iter()
        .map(|(name, value)| HttpHeader { name: name.as_str().to_string(), value: value.as_bytes().into() })
        .collect()
}

/// Headers that crossed, back as a map; a name or value HTTP doesn't allow is an error.
pub fn headers_from_wire(headers: &[HttpHeader]) -> Result<http::HeaderMap> {
    let mut map = http::HeaderMap::with_capacity(headers.len());
    for header in headers {
        let name = http::HeaderName::from_bytes(header.name.as_bytes())
            .with_context(|| format!("{:?} isn't an HTTP header name", header.name))?;
        let value = http::HeaderValue::from_bytes(&header.value)
            .with_context(|| format!("the {:?} header's value isn't one HTTP allows", header.name))?;
        map.append(name, value);
    }
    Ok(map)
}

impl HttpRequestHead {
    pub fn from_parts(parts: &http::request::Parts) -> Self {
        HttpRequestHead {
            method: parts.method.as_str().to_string(),
            uri: parts.uri.to_string(),
            http2: parts.version == http::Version::HTTP_2,
            headers: headers_to_wire(&parts.headers),
            length: None,
        }
    }

    /// The request, with `body`.
    pub fn to_request<B>(&self, body: B) -> Result<http::Request<B>> {
        let mut request = http::Request::new(body);
        *request.method_mut() = http::Method::from_bytes(self.method.as_bytes())
            .with_context(|| format!("{:?} isn't an HTTP method", self.method))?;
        *request.uri_mut() = self.uri.parse().with_context(|| format!("{:?} isn't a URL", self.uri))?;
        if self.http2 {
            *request.version_mut() = http::Version::HTTP_2;
        }
        *request.headers_mut() = headers_from_wire(&self.headers)?;
        Ok(request)
    }
}

impl HttpResponseHead {
    pub fn from_parts(parts: &http::response::Parts) -> Self {
        HttpResponseHead { status: parts.status.as_u16(), headers: headers_to_wire(&parts.headers) }
    }

    /// The response, with `body`.
    pub fn to_response<B>(&self, body: B) -> Result<http::Response<B>> {
        let mut response = http::Response::new(body);
        *response.status_mut() =
            http::StatusCode::from_u16(self.status).with_context(|| format!("{} isn't an HTTP status", self.status))?;
        *response.headers_mut() = headers_from_wire(&self.headers)?;
        Ok(response)
    }
}

impl From<http::Request<Vec<u8>>> for HttpHit {
    fn from(request: http::Request<Vec<u8>>) -> Self {
        let (parts, body) = request.into_parts();
        HttpHit { head: HttpRequestHead::from_parts(&parts), body: body.into() }
    }
}

impl From<http::Response<Vec<u8>>> for HttpReply {
    fn from(response: http::Response<Vec<u8>>) -> Self {
        let (parts, body) = response.into_parts();
        HttpReply { head: HttpResponseHead::from_parts(&parts), body: body.into() }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_and_responses_cross_and_come_back() {
        let request = http::Request::post("https://example.com/v1?page=2")
            .version(http::Version::HTTP_2)
            .header("content-type", "application/grpc")
            .header("x-bin", &[0xe9, 0xff][..])
            .body(b"hi".to_vec())
            .unwrap();
        let hit = HttpHit::from(request);
        assert!(hit.head.http2);
        let back = hit.head.to_request(hit.body.0.clone()).unwrap();
        assert_eq!(back.method(), http::Method::POST);
        assert_eq!(back.uri(), "https://example.com/v1?page=2");
        assert_eq!(back.headers()["x-bin"].as_bytes(), [0xe9, 0xff]);
        assert_eq!(back.body(), b"hi");

        let reply = HttpReply::from(http::Response::builder().status(404).body(Vec::new()).unwrap());
        assert_eq!(reply.head.to_response(()).unwrap().status(), 404);
        assert!(HttpResponseHead { status: 1000, headers: Vec::new() }.to_response(()).is_err());
    }
}
