use std::sync::Arc;

use pyo3::exceptions::{PyConnectionError, PyEOFError};
use pyo3::prelude::*;
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

/// Internal message exchanged between the Rust bridging task and the Python
/// `WebSocket` object.
pub enum WsMessage {
    Text(String),
    Bytes(Vec<u8>),
    Close,
}

/// A WebSocket connection exposed to Python handlers.
///
/// Instances are created by JustAPIApp when a WebSocket upgrade is accepted and
/// passed as the single argument to a `@app.websocket()` handler:
///
/// ```python
/// @app.websocket("/ws")
/// async def handler(ws):
///     await ws.accept()
///     await ws.send_text("hello")
///     msg = await ws.receive_text()
///     await ws.close()
/// ```
///
/// All methods are async and integrate with the daemon asyncio event loop via
/// `pyo3-async-runtimes`, so they can be `await`ed directly inside an async
/// handler. The actual bytes cross the PyO3 boundary through lock-free channels
/// that the Rust side bridges to the tokio-tungstenite stream.
#[pyclass]
pub struct WebSocket {
    incoming: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<WsMessage>>>,
    outgoing: UnboundedSender<WsMessage>,
}

impl WebSocket {
    pub fn new(
        incoming: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<WsMessage>>>,
        outgoing: UnboundedSender<WsMessage>,
    ) -> Self {
        Self { incoming, outgoing }
    }
}

#[pymethods]
impl WebSocket {
    /// Complete the handshake. The Rust side has already accepted the upgrade,
    /// so this is effectively a no-op kept for API parity with other frameworks.
    fn accept<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        pyo3_async_runtimes::tokio::future_into_py(py, async { Ok(()) })
    }

    /// Receive the next message as a `str`/`bytes`, or `None` when the peer
    /// closes the connection.
    fn receive<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let incoming = self.incoming.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut rx = incoming.lock().await;
            match rx.recv().await {
                Some(WsMessage::Text(t)) => Ok(Some(t)),
                Some(WsMessage::Bytes(b)) => Ok(Some(String::from_utf8_lossy(&b).into_owned())),
                Some(WsMessage::Close) | None => Ok(None),
            }
        })
    }

    /// Receive the next message as text, raising `EOFError` on close.
    fn receive_text<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let incoming = self.incoming.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut rx = incoming.lock().await;
            match rx.recv().await {
                Some(WsMessage::Text(t)) => Ok(t),
                Some(WsMessage::Bytes(b)) => Ok(String::from_utf8_lossy(&b).into_owned()),
                Some(WsMessage::Close) | None => {
                    Err(PyEOFError::new_err("websocket connection closed"))
                }
            }
        })
    }

    /// Receive the next message as raw bytes, raising `EOFError` on close.
    fn receive_bytes<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let incoming = self.incoming.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let mut rx = incoming.lock().await;
            match rx.recv().await {
                Some(WsMessage::Text(t)) => Ok(t.into_bytes()),
                Some(WsMessage::Bytes(b)) => Ok(b),
                Some(WsMessage::Close) | None => {
                    Err(PyEOFError::new_err("websocket connection closed"))
                }
            }
        })
    }

    /// Send a message. `bytes` are sent as binary.
    fn send<'py>(&self, py: Python<'py>, message: &[u8]) -> PyResult<Bound<'py, PyAny>> {
        self.send_bytes(py, message)
    }

    /// Send a UTF-8 text message.
    fn send_text<'py>(&self, py: Python<'py>, message: String) -> PyResult<Bound<'py, PyAny>> {
        let outgoing = self.outgoing.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            outgoing
                .send(WsMessage::Text(message))
                .map_err(|_| PyConnectionError::new_err("websocket connection closed"))?;
            Ok(())
        })
    }

    /// Send a binary message.
    fn send_bytes<'py>(&self, py: Python<'py>, message: &[u8]) -> PyResult<Bound<'py, PyAny>> {
        let outgoing = self.outgoing.clone();
        let msg = message.to_vec();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            outgoing
                .send(WsMessage::Bytes(msg))
                .map_err(|_| PyConnectionError::new_err("websocket connection closed"))?;
            Ok(())
        })
    }

    /// Send a close frame and terminate the connection.
    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let outgoing = self.outgoing.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let _ = outgoing.send(WsMessage::Close);
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        "WebSocket()".to_string()
    }
}
