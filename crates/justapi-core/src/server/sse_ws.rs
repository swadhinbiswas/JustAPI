//! SSE response and WebSocket dispatch helpers.

use hyper::body::Bytes;
use hyper::{Response, StatusCode};

use crate::{streaming_response, ResponseBody};

#[cfg(feature = "ws")]
use super::{WsConnInfo, WsHandler, WsRead, WsWrite};

/// Build a demo SSE endpoint that sends 10 events at 100ms intervals.
pub fn sse_response() -> Response<ResponseBody> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes, anyhow::Error>>(16);

    tokio::spawn(async move {
        for i in 1..=10 {
            let msg = format!("data: {{\"count\":{}}}\n\n", i);
            if tx.send(Ok(Bytes::from(msg))).await.is_err() {
                break;
            }
            tokio::time::sleep(std::time::Duration::from_millis(100)).await;
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
