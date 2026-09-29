//! TEMPORARY(network): the app's HTTP for plugins, headless: the fixture fetches from
//! a small local HTTP server, listens for a request this test sends it, and makes gRPC
//! calls (unary and server-streaming) to a small local HTTP/2 server; and without the
//! `Network` permission it can't. Nothing leaves this Mac.

use std::convert::Infallible;
use std::io::{BufRead as _, BufReader, Read as _, Write as _};
use std::net::{TcpListener, TcpStream};
use std::time::{Duration, Instant};

use super::*;

/// An HTTP/1.1 server on a thread: it answers each request with "got {method} {path}
/// {body} {x-fixture}", in two chunks.
fn http_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            reader.read_line(&mut line).unwrap();
            let mut parts = line.split_whitespace();
            let (method, path) = (parts.next().unwrap().to_string(), parts.next().unwrap().to_string());
            let (mut length, mut marked) = (0, String::new());
            loop {
                let mut header = String::new();
                reader.read_line(&mut header).unwrap();
                let header = header.trim_end();
                if header.is_empty() {
                    break;
                }
                let (name, value) = header.split_once(':').unwrap();
                match name.to_ascii_lowercase().as_str() {
                    "content-length" => length = value.trim().parse().unwrap(),
                    "x-fixture" => marked = value.trim().to_string(),
                    _ => {}
                }
            }
            let mut body = vec![0; length];
            reader.read_exact(&mut body).unwrap();
            let rest = format!("{method} {path} {} {marked}", String::from_utf8_lossy(&body));
            let response = format!(
                "HTTP/1.1 200 OK\r\ntransfer-encoding: chunked\r\nconnection: close\r\n\r\n4\r\ngot \r\n{:x}\r\n{rest}\r\n0\r\n\r\n",
                rest.len()
            );
            stream.write_all(response.as_bytes()).unwrap();
        }
    });
    port
}

#[gpui::test]
async fn a_plugin_fetches_through_the_apps_http(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let port = http_server();
    let (plugin, app) = start("http", cx).await;
    let tool = tool_with(&plugin, &format!("http://127.0.0.1:{port}/hello?x=1"), cx).await;
    cx.update(|cx| drop(tool.perform_action("Fetch".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, "200 got POST /hello?x=1 ping 1");
}

/// A server that answers `/ok` and holds every other request open, unanswered.
fn hanging_server() -> (u16, std::sync::Arc<std::sync::atomic::AtomicUsize>) {
    let hung = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let counted = hung.clone();
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let mut held = Vec::new();
        for stream in listener.incoming() {
            let mut stream = stream.unwrap();
            let mut head = String::new();
            let mut reader = BufReader::new(stream.try_clone().unwrap());
            let mut line = String::new();
            while reader.read_line(&mut line).unwrap_or(0) > 2 {
                head.push_str(&line);
                line.clear();
            }
            if head.starts_with("GET /ok") {
                stream.write_all(b"HTTP/1.1 200 OK\r\ncontent-length: 2\r\nconnection: close\r\n\r\nok").unwrap();
            } else {
                counted.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                held.push(stream);
            }
        }
    });
    (port, hung)
}

/// Wait until `count` requests reached the hanging server (running the app meanwhile).
fn wait_for_hung(hung: &std::sync::atomic::AtomicUsize, count: usize, cx: &mut TestAppContext) {
    let deadline = Instant::now() + Duration::from_secs(20);
    while hung.load(std::sync::atomic::Ordering::SeqCst) < count {
        assert!(Instant::now() < deadline, "only {} requests reached the server", hung.load(std::sync::atomic::Ordering::SeqCst));
        cx.executor().run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
}

/// A request that has finished no longer counts as open: a plugin makes far more than the most
/// it may have open at once, one after another.
#[gpui::test]
async fn finished_requests_stop_counting_as_open(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (port, _hung) = hanging_server();
    let (plugin, app) = start("finished-requests", cx).await;
    let tool = tool_with(&plugin, &format!("http://127.0.0.1:{port}"), cx).await;
    for n in 0..40 {
        app.update(cx, |app, _| app.toasts.clear());
        cx.update(|cx| drop(tool.perform_action("Ok".into(), cx)));
        let toast = wait_for_toast(&app, cx, |toast| toast.starts_with("ok"));
        assert_eq!(toast, "ok 200", "request {n}");
    }
}

/// Nor does one the plugin gave up on (dropped the task of) while it was still open.
#[gpui::test]
async fn requests_a_plugin_gives_up_on_stop_counting_as_open(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (port, hung) = hanging_server();
    let (plugin, app) = start("giving-up", cx).await;
    let tool = tool_with(&plugin, &format!("http://127.0.0.1:{port}"), cx).await;
    // As many open as it may (a few more than that would be refused), given up on, over and over.
    for round in 1..=4 {
        for _ in 0..8 {
            cx.update(|cx| drop(tool.perform_action("Hang".into(), cx)));
        }
        wait_for_hung(&hung, round * 8, cx);
        cx.update(|cx| drop(tool.perform_action("Release".into(), cx)));
        cx.executor().run_until_parked();
    }
    cx.update(|cx| drop(tool.perform_action("Ok".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| toast.starts_with("ok"));
    assert_eq!(toast, "ok 200");
}

#[gpui::test]
async fn a_plugin_answers_requests_to_its_listener(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let (plugin, app) = start("listen", cx).await;
    let tool = tool_with(&plugin, "", cx).await;
    cx.update(|cx| drop(tool.perform_action("Listen".into(), cx)));
    let listening = wait_for_toast(&app, cx, |toast| toast.starts_with("listening "));
    let port: u16 = listening["listening ".len()..].parse().unwrap();

    // The browser coming back from signing in, on another thread: the plugin answers
    // on this one.
    let browser = std::thread::spawn(move || {
        let mut stream = TcpStream::connect(("127.0.0.1", port)).unwrap();
        stream.write_all(b"GET /callback?code=42 HTTP/1.1\r\nhost: 127.0.0.1\r\nconnection: close\r\n\r\n").unwrap();
        let mut response = String::new();
        stream.read_to_string(&mut response).unwrap();
        response
    });
    wait_for_toast(&app, cx, |toast| toast == "hit /callback?code=42");
    let deadline = Instant::now() + Duration::from_secs(20);
    while !browser.is_finished() {
        assert!(Instant::now() < deadline, "the listener didn't answer");
        cx.executor().run_until_parked();
        std::thread::sleep(Duration::from_millis(10));
    }
    let response = browser.join().unwrap();
    assert!(response.starts_with("HTTP/1.1 200"), "{response}");
    assert!(response.ends_with("hello /callback?code=42"), "{response}");
}

/// A gRPC server speaking HTTP/2 without TLS, on its own runtime: `echo.Echo/Say`
/// answers "said {text}", and `echo.Echo/Count` streams "1" to "{text}". gRPC's
/// framing (a 5-byte prefix) and its one-field messages are written out by hand.
fn grpc_server() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    listener.set_nonblocking(true).unwrap();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build().unwrap();
        runtime.block_on(async move {
            let listener = tokio::net::TcpListener::from_std(listener).unwrap();
            loop {
                let (stream, _) = listener.accept().await.unwrap();
                tokio::spawn(async move {
                    let service = hyper::service::service_fn(answer_grpc);
                    let builder = hyper::server::conn::http2::Builder::new(hyper_util::rt::TokioExecutor::new());
                    drop(builder.serve_connection(hyper_util::rt::TokioIo::new(stream), service).await);
                });
            }
        });
    });
    port
}

type GrpcBody = http_body_util::StreamBody<futures::stream::Iter<std::vec::IntoIter<Result<hyper::body::Frame<bytes::Bytes>, Infallible>>>>;

async fn answer_grpc(request: http::Request<hyper::body::Incoming>) -> Result<http::Response<GrpcBody>, Infallible> {
    use http_body_util::BodyExt as _;
    let path = request.uri().path().to_string();
    let body = request.into_body().collect().await.unwrap().to_bytes();
    let text = decode(&body[5..]);
    let messages: Vec<String> = match path.as_str() {
        "/echo.Echo/Say" => vec![format!("said {text}")],
        "/echo.Echo/Count" => (1..=text.parse::<u32>().unwrap()).map(|n| n.to_string()).collect(),
        _ => Vec::new(),
    };
    let mut frames: Vec<_> = messages.iter().map(|message| Ok(hyper::body::Frame::data(framed(message)))).collect();
    let mut trailers = http::HeaderMap::new();
    trailers.insert("grpc-status", http::HeaderValue::from_static("0"));
    frames.push(Ok(hyper::body::Frame::trailers(trailers)));
    let mut response = http::Response::new(http_body_util::StreamBody::new(futures::stream::iter(frames)));
    response.headers_mut().insert("content-type", http::HeaderValue::from_static("application/grpc"));
    Ok(response)
}

/// A message's one string field (tag 1), short enough for a one-byte length.
fn decode(message: &[u8]) -> String {
    assert_eq!(message[0], 0x0a, "field 1, a string");
    String::from_utf8(message[2..2 + message[1] as usize].to_vec()).unwrap()
}

/// `text` as a one-field message in gRPC's framing: not compressed, then its length.
fn framed(text: &str) -> bytes::Bytes {
    let message = [&[0x0a, text.len() as u8][..], text.as_bytes()].concat();
    let mut frame = vec![0];
    frame.extend((message.len() as u32).to_be_bytes());
    frame.extend(message);
    frame.into()
}

#[gpui::test]
async fn a_plugin_makes_grpc_calls_with_tonics_client(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let port = grpc_server();
    let (plugin, app) = start("grpc", cx).await;
    let tool = tool_with(&plugin, &format!("http://127.0.0.1:{port}"), cx).await;
    cx.update(|cx| drop(tool.perform_action("Grpc".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, "said hi 1,2,3");
}

#[gpui::test]
async fn without_network_a_plugin_has_no_http(cx: &mut TestAppContext) {
    cx.executor().allow_parking();
    let wasm = fixture();
    let mut manifest = read_manifest(&std::fs::read(&wasm).unwrap()).unwrap();
    manifest.plugin.permissions.clear();
    let options = plugin_options(&manifest, data_dir("no-network"), Arc::new(gpui::NoopTextSystem::new()));
    let app = cx.new(|_| FakeApp::default());
    let started = cx.update(|cx| Plugin::start(wasm, manifest, options, data_dir("no-network"), root_of(&app), cx));
    settle(cx);
    let plugin = started.await.expect("starts");
    let tool = tool_with(&plugin, "http://127.0.0.1:9/", cx).await;
    cx.update(|cx| drop(tool.perform_action("Fetch".into(), cx)));
    let toast = wait_for_toast(&app, cx, |toast| !toast.is_empty());
    assert_eq!(toast, "this plugin doesn't have the Network permission");
}
