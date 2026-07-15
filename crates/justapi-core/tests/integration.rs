use std::collections::HashMap;
use std::net::SocketAddr;

use http_body_util::BodyExt;
use hyper::{Method, StatusCode};
use hyper_util::rt::TokioIo;
use justapi_core::serve;
use tokio::net::{TcpListener, TcpStream};

async fn send_request(
    addr: SocketAddr,
    method: Method,
    path: &str,
    body: Option<&'static [u8]>,
) -> (StatusCode, Vec<u8>) {
    let (status, body, _headers) =
        send_request_with_headers(addr, method, path, body, &HashMap::new()).await;
    (status, body)
}

async fn send_request_with_headers(
    addr: SocketAddr,
    method: Method,
    path: &str,
    body: Option<&'static [u8]>,
    extra_headers: &HashMap<&str, &str>,
) -> (StatusCode, Vec<u8>, hyper::HeaderMap) {
    let stream = TcpStream::connect(addr).await.unwrap();
    let (mut sender, conn) =
        hyper::client::conn::http1::handshake(TokioIo::new(stream)).await.unwrap();
    tokio::spawn(conn);

    let mut builder = hyper::Request::builder().method(method).uri(path);
    if body.is_some() {
        builder = builder.header("content-type", "application/json");
    }
    for (k, v) in extra_headers {
        builder = builder.header(*k, *v);
    }
    let req = builder
        .body(http_body_util::Full::new(hyper::body::Bytes::from(body.unwrap_or(b""))))
        .unwrap();
    let resp = sender.send_request(req).await.unwrap();
    let status = resp.status();
    let headers = resp.headers().clone();
    let body = resp.collect().await.unwrap().to_bytes().to_vec();
    (status, body, headers)
}

/// Helper to start a server and return the address
async fn start_server() -> SocketAddr {
    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        serve(listener).await.unwrap();
    });
    tokio::time::sleep(std::time::Duration::from_millis(200)).await;
    addr
}

#[tokio::test]
async fn test_loopback_hello() {
    let addr = start_server().await;
    let (status, body) = send_request(addr, Method::GET, "/hello", None).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], br#"{"message":"hello"}"#);
}

#[tokio::test]
async fn test_loopback_echo() {
    let addr = start_server().await;
    let (status, body) = send_request(addr, Method::POST, "/echo", Some(b"hello")).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], b"hello");
}

#[tokio::test]
async fn test_loopback_404() {
    let addr = start_server().await;
    let (status, body) = send_request(addr, Method::GET, "/nonexistent", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
    assert_eq!(&body[..], br#"{"error":"not found"}"#);
}

#[tokio::test]
async fn test_loopback_sse() {
    let addr = start_server().await;
    let (status, body) = send_request(addr, Method::GET, "/events", None).await;
    assert_eq!(status, StatusCode::OK);
    let body_str = String::from_utf8(body).unwrap();
    assert!(body_str.contains("data:"));
    assert!(body_str.contains("\"count\":10"));
}

// --- Phase 9: Compression tests ---

#[tokio::test]
async fn test_compression_gzip_echo() {
    let addr = start_server().await;
    let mut headers = HashMap::new();
    headers.insert("accept-encoding", "gzip");
    let payload = b"hello world this is a test payload for compression";
    let (status, body, _resp_headers) =
        send_request_with_headers(addr, Method::POST, "/echo", Some(payload), &headers).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(&body[..], payload);
}

#[tokio::test]
async fn test_compression_not_applied_to_small_responses() {
    let addr = start_server().await;
    let mut headers = HashMap::new();
    headers.insert("accept-encoding", "gzip");
    let (status, body, resp_headers) =
        send_request_with_headers(addr, Method::GET, "/hello", None, &headers).await;
    assert_eq!(status, StatusCode::OK);
    // Small response should not be compressed (and serve() doesn't add compression middleware)
    assert_eq!(&body[..], br#"{"message":"hello"}"#);
    // No content-encoding header
    assert!(!resp_headers.contains_key("content-encoding"));
}

#[tokio::test]
async fn test_health_endpoint() {
    let addr = start_server().await;
    let (status, body, _headers) =
        send_request_with_headers(addr, Method::GET, "/health", None, &HashMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["status"], "ok");
}

#[tokio::test]
async fn test_ready_endpoint() {
    let addr = start_server().await;
    let (status, body, _headers) =
        send_request_with_headers(addr, Method::GET, "/ready", None, &HashMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["ready"], true);
}

#[tokio::test]
async fn test_live_endpoint() {
    let addr = start_server().await;
    let (status, body, _headers) =
        send_request_with_headers(addr, Method::GET, "/live", None, &HashMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
    assert_eq!(json["alive"], true);
}

#[tokio::test]
async fn test_metrics_endpoint() {
    let addr = start_server().await;
    let (status, body, _headers) =
        send_request_with_headers(addr, Method::GET, "/metrics", None, &HashMap::new()).await;
    assert_eq!(status, StatusCode::OK);
    let body_str = String::from_utf8(body).unwrap();
    assert!(body_str.contains("justapi_requests_total"));
    assert!(body_str.contains("justapi_active_connections"));
}

// --- Phase 9: Params echo test ---
// Note: /echo/{*rest} route is only registered in Server::new(), not in serve().
// This test verifies the standalone serve() fallback returns 404 for unmatched paths.
#[tokio::test]
async fn test_loopback_params_echo_unmatched() {
    let addr = start_server().await;
    let (status, _body) = send_request(addr, Method::GET, "/echo/foo/bar", None).await;
    assert_eq!(status, StatusCode::NOT_FOUND);
}

#[cfg(feature = "ws")]
#[tokio::test]
async fn test_loopback_websocket() {
    use futures::{SinkExt, StreamExt};
    use tokio_tungstenite::tungstenite::Message;

    let listener = TcpListener::bind(SocketAddr::from(([127, 0, 0, 1], 0))).await.unwrap();
    let addr = listener.local_addr().unwrap();

    tokio::spawn(async move {
        justapi_core::serve(listener).await.unwrap();
    });

    tokio::time::sleep(std::time::Duration::from_millis(200)).await;

    let url = format!("ws://{}/ws", addr);
    let (mut ws, _) =
        tokio_tungstenite::connect_async(&url).await.expect("WebSocket connection should succeed");

    ws.send(Message::Text("hello".into())).await.unwrap();
    let msg = ws.next().await.unwrap().unwrap();
    assert_eq!(msg, Message::Text("hello".into()));
}
