//! TEMPORARY(network): the fixture's network actions, which the headless tests
//! perform: HTTP to the URL in the input, a listener on this Mac, and gRPC (a unary
//! call and a server-streaming one) to the server in the input. Each toasts what came
//! back.

use delight_plugin_api::gpui::{Context, Task};
use delight_plugin_api::network::grpc;
use delight_plugin_api::tonic;
use delight_plugin_api::{host, http};

use super::Echo;

/// POST a small body to the input's URL, and toast "{status} {body}".
pub fn fetch(url: &str, cx: &mut Context<Echo>) {
    let request = http::Request::post(url).header("x-fixture", "1").body(b"ping".to_vec());
    let request = match request {
        Ok(request) => request,
        Err(error) => return host(cx).toast(format!("{error}"), cx),
    };
    let sent = host(cx).http(request, cx);
    cx.spawn(async move |_, cx| {
        let message = match sent.await {
            Ok(response) => format!("{} {}", response.status().as_u16(), String::from_utf8_lossy(response.body())),
            Err(error) => format!("{error:#}"),
        };
        cx.update(|cx| host(cx).toast(message, cx));
    })
    .detach();
}

/// A request to `{url}/hang`, which is never answered; toasts what became of it (only if it ends).
pub fn hang(url: &str, cx: &mut Context<Echo>) -> Task<()> {
    let request = http::Request::get(format!("{url}/hang")).body(Vec::new()).unwrap();
    let sent = host(cx).http(request, cx);
    cx.spawn(async move |_, cx| {
        let message = match sent.await {
            Ok(response) => format!("hang {}", response.status().as_u16()),
            Err(error) => format!("hang: {error:#}"),
        };
        cx.update(|cx| host(cx).toast(message, cx));
    })
}

/// One request to `{url}/ok`; toasts "ok {status}" or the error.
pub fn ok(url: &str, cx: &mut Context<Echo>) {
    let request = http::Request::get(format!("{url}/ok")).body(Vec::new()).unwrap();
    let sent = host(cx).http(request, cx);
    cx.spawn(async move |_, cx| {
        let message = match sent.await {
            Ok(response) => format!("ok {}", response.status().as_u16()),
            Err(error) => format!("ok: {error:#}"),
        };
        cx.update(|cx| host(cx).toast(message, cx));
    })
    .detach();
}

/// Listen on any free port, answer each request with "hello {path}" (toasting "hit
/// {path}"), and toast "listening {port}".
pub fn listen(cx: &mut Context<Echo>) {
    let listening = host(cx).listen_http(
        0,
        |request, cx| {
            let path = request.uri().to_string();
            host(cx).toast(format!("hit {path}"), cx);
            Task::ready(http::Response::new(format!("hello {path}").into_bytes()))
        },
        cx,
    );
    cx.spawn(async move |this, cx| match listening.await {
        Ok(listener) => {
            let port = listener.port();
            this.update(cx, |echo, cx| {
                echo.listener = Some(listener);
                host(cx).toast(format!("listening {port}"), cx);
            })
            .ok();
        }
        Err(error) => cx.update(|cx| host(cx).toast(format!("{error:#}"), cx)),
    })
    .detach();
}

/// A message of the tests' gRPC service: one string.
#[derive(Clone, PartialEq, prost::Message)]
pub struct Text {
    #[prost(string, tag = "1")]
    pub text: String,
}

/// Call the tests' `echo.Echo` service at the input's URL: `Say` ("hi"), then `Count`
/// ("3", streamed back as "1", "2", "3"). Toasts "{said} {counted}".
pub fn grpc(base: &str, cx: &mut Context<Echo>) {
    let base = match base.parse::<http::Uri>() {
        Ok(base) => base,
        Err(error) => return host(cx).toast(format!("{error}"), cx),
    };
    let channel = grpc::channel(base, cx);
    cx.spawn(async move |_, cx| {
        let message = calls(channel).await.unwrap_or_else(|status| format!("gRPC {status}"));
        cx.update(|cx| host(cx).toast(message, cx));
    })
    .detach();
}

/// The two calls, as tonic's generated clients make them.
async fn calls(channel: grpc::Channel) -> Result<String, tonic::Status> {
    let mut client = tonic::client::Grpc::new(channel);
    client.ready().await.map_err(|error| tonic::Status::unknown(error.to_string()))?;
    let say = http::uri::PathAndQuery::from_static("/echo.Echo/Say");
    let codec = tonic_prost::ProstCodec::<Text, Text>::default();
    let said = client.unary(tonic::Request::new(Text { text: "hi".into() }), say, codec).await?.into_inner().text;
    let count = http::uri::PathAndQuery::from_static("/echo.Echo/Count");
    let codec = tonic_prost::ProstCodec::<Text, Text>::default();
    let mut counted = client.server_streaming(tonic::Request::new(Text { text: "3".into() }), count, codec).await?.into_inner();
    let mut numbers = Vec::new();
    while let Some(number) = counted.message().await? {
        numbers.push(number.text);
    }
    Ok(format!("{said} {}", numbers.join(",")))
}
