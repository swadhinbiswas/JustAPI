//! WebSocket handler types and default echo implementation.

#[cfg(feature = "ws")]
pub type WsRead = Box<
    dyn futures::Stream<
            Item = Result<
                tokio_tungstenite::tungstenite::Message,
                tokio_tungstenite::tungstenite::Error,
            >,
        > + Unpin
        + Send,
>;

#[cfg(feature = "ws")]
pub type WsWrite = Box<
    dyn futures::Sink<
            tokio_tungstenite::tungstenite::Message,
            Error = tokio_tungstenite::tungstenite::Error,
        > + Unpin
        + Send,
>;

/// Connection metadata handed to a WebSocket handler on upgrade. Mirrors the
/// subset of the HTTP request a WebSocket scope needs (`path`, decoded query
/// string, raw headers) so handler frameworks can build a Starlette-style
/// `scope` without re-reading the (already upgraded) socket.
#[cfg(feature = "ws")]
#[derive(Debug, Clone)]
pub struct WsConnInfo {
    /// Request path the upgrade arrived on.
    pub path: String,
    /// Raw query string (percent-encoded, without the leading `?`).
    pub query_string: Vec<u8>,
    /// Raw request headers observed on the upgrade request.
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
    /// Remote peer address as `(host, port)`, if known.
    pub client: Option<(String, u16)>,
}

#[cfg(feature = "ws")]
pub type WsHandler = std::sync::Arc<
    dyn Fn(
            WsConnInfo,
            WsRead,
            WsWrite,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

/// Default WebSocket handler used when no application handler is registered.
/// Echoes text/binary frames (mirroring the legacy raw-TCP behavior) so that
/// standalone servers remain WebSocket-compatible out of the box. `with_ws`
/// replaces this with an application-provided handler.
#[cfg(feature = "ws")]
pub fn default_ws_echo() -> WsHandler {
    std::sync::Arc::new(|_info, mut read, mut write| {
        Box::pin(async move {
            use futures::{SinkExt, StreamExt};
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(m @ tokio_tungstenite::tungstenite::Message::Text(_))
                    | Ok(m @ tokio_tungstenite::tungstenite::Message::Binary(_)) => {
                        if write.send(m).await.is_err() {
                            break;
                        }
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Ping(p)) => {
                        let _ = write.send(tokio_tungstenite::tungstenite::Message::Pong(p)).await;
                    }
                    Ok(tokio_tungstenite::tungstenite::Message::Close(_)) | Err(_) => {
                        let _ =
                            write.send(tokio_tungstenite::tungstenite::Message::Close(None)).await;
                        break;
                    }
                    _ => {}
                }
            }
            let _ = write.close().await;
            Ok(())
        })
    })
}
