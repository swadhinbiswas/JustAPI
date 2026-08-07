//! SSE response and WebSocket dispatch helpers.

use hyper::body::Bytes;
use hyper::{Response, StatusCode};

use crate::{streaming_response, ResponseBody};

#[cfg(feature = "ws")]
use super::{WsConnInfo, WsHandler, WsRead, WsWrite};

/// Build a demo SSE endpoint that sends 10 events at 100ms intervals.
pub fn sse_response() -> Response<ResponseBody> {
    sse_stream_response(10, 100)
}

/// Rust-native SSE stream: emits `count` events at `interval_ms` spacing,
/// entirely in Rust (tokio + mpsc + stream). No Python, no GIL — this is the
/// "server runs on Rust" streaming path (ADR-088).
///
/// Each event is `data: {"n":<i>}\n\n`. The producer task runs on the tokio
/// runtime; the response body is an mpsc-backed stream the HTTP layer drains.
pub fn sse_stream_response(count: u64, interval_ms: u64) -> Response<ResponseBody> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, anyhow::Error>>(16);

    tokio::spawn(async move {
        for i in 1..=count {
            let msg = format!("data: {{\"n\":{}}}\n\n", i);
            if tx.send(Ok(Bytes::from(msg))).await.is_err() {
                break;
            }
            if interval_ms > 0 {
                tokio::time::sleep(std::time::Duration::from_millis(interval_ms)).await;
            }
        }
    });

    let stream = tokio_stream::wrappers::ReceiverStream::new(rx);
    streaming_response(StatusCode::OK, "text/event-stream", stream)
}

/// Dispatch an accepted WebSocket connection to a registered handler.
#[cfg(feature = "ws")]
pub async fn dispatch_ws(read: WsRead, write: WsWrite, info: WsConnInfo, handler: &WsHandler) {
    if let Err(e) = handler(info, read, write).await {
        tracing::warn!("WebSocket handler error: {}", e);
    }
}
