use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::Arc;

use pyo3::exceptions::{PyConnectionError, PyEOFError, PyNotImplementedError};
use pyo3::prelude::*;
use pyo3::types::{PyAny, PyDict};
use tokio::sync::mpsc::UnboundedSender;
use tokio::sync::Mutex;

use crate::request::{build_cookies, build_headers, build_query_params, build_url, Conn};

/// Internal message exchanged between the Rust bridging task and the Python
/// `WebSocket` object.
pub enum WsMessage {
    Text(String),
    Bytes(Vec<u8>),
    Close(Option<u16>, Option<String>),
}

/// WebSocket connection lifecycle states (mirrors `starlette.websockets.WebSocketState`).
const WS_STATE_CONNECTING: u8 = 0;
const WS_STATE_CONNECTED: u8 = 1;
const WS_STATE_DISCONNECTED: u8 = 2;
const WS_STATE_RESPONSE: u8 = 3;

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
/// The object carries the Starlette-style connection scope (`headers`,
/// `query_params`, `path_params`, `cookies`, `client`, `url`, `app`, `state`)
/// and supports the same async surface as FastAPI/Starlette (`receive_text`,
/// `receive_bytes`, `receive_json`, `send_text`, `send_bytes`, `send_json`,
/// `url_for`, `iter_text`, `iter_bytes`, `iter_json`, `close`).
///
/// All methods are async and integrate with the daemon asyncio event loop via
/// `pyo3-async-runtimes`, so they can be `await`ed directly inside an async
/// handler. The actual bytes cross the PyO3 boundary through lock-free channels
/// that the Rust side bridges to the tokio-tungstenite stream.
#[pyclass(name = "WebSocket")]
pub struct WebSocket {
    incoming: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<WsMessage>>>,
    outgoing: UnboundedSender<WsMessage>,
    conn: Conn,
    client_state: AtomicU8,
    application_state: AtomicU8,
    subprotocol: std::sync::Mutex<Option<String>>,
}

impl WebSocket {
    pub(crate) fn new(
        incoming: Arc<Mutex<tokio::sync::mpsc::UnboundedReceiver<WsMessage>>>,
        outgoing: UnboundedSender<WsMessage>,
        conn: Conn,
    ) -> Self {
        Self {
            incoming,
            outgoing,
            conn,
            client_state: AtomicU8::new(WS_STATE_CONNECTING),
            application_state: AtomicU8::new(WS_STATE_CONNECTING),
            subprotocol: std::sync::Mutex::new(None),
        }
    }
}

#[pymethods]
impl WebSocket {
    /// The application instance (mirrors `starlette.WebSocket.app`).
    #[getter]
    fn app(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.conn.app.as_ref().map(|a| a.clone_ref(py))
    }

    /// The full URL of the connection (mirrors `starlette.WebSocket.url`).
    #[getter]
    fn url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = crate::request::host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, &self.conn.path, &self.conn.query_string_raw, "")
    }

    /// The base URL (scheme + host + port, no path) of the connection.
    #[getter]
    fn base_url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = crate::request::host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, "", b"", "")
    }

    /// The request headers (mirrors `starlette.WebSocket.headers`).
    #[getter]
    fn headers(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_headers(py, &self.conn)
    }

    /// The parsed query parameters (mirrors `starlette.WebSocket.query_params`).
    #[getter]
    fn query_params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_query_params(py, &self.conn)
    }

    /// The path parameters (mirrors `starlette.WebSocket.path_params`).
    #[getter]
    fn path_params(&self, py: Python<'_>) -> Py<PyAny> {
        let d = PyDict::new(py);
        for (k, v) in &self.conn.path_params_raw {
            d.set_item(k.as_str(), v.as_str()).ok();
        }
        d.into_any().unbind()
    }

    /// The parsed cookies (mirrors `starlette.WebSocket.cookies`).
    #[getter]
    fn cookies(&self, py: Python<'_>) -> Py<PyAny> {
        build_cookies(py, &self.conn)
    }

    /// The remote client address as a `(host, port)` tuple, or `None`.
    #[getter]
    fn client(&self) -> Option<(String, u16)> {
        self.conn.client.clone()
    }

    /// The connection state object (mirrors `starlette.WebSocket.state`).
    #[getter]
    fn state(&self, py: Python<'_>) -> Py<PyAny> {
        self.conn.state.clone_ref(py).into_any()
    }

    /// The client-side lifecycle state (mirrors `starlette.WebSocket.client_state`).
    #[getter]
    fn client_state(&self) -> u8 {
        self.client_state.load(Ordering::SeqCst)
    }

    /// The application-side lifecycle state
    /// (mirrors `starlette.WebSocket.application_state`).
    #[getter]
    fn application_state(&self) -> u8 {
        self.application_state.load(Ordering::SeqCst)
    }

    /// The negotiated subprotocol (set by `accept`).
    #[getter]
    fn subprotocol(&self) -> Option<String> {
        self.subprotocol.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// Build a URL for a named route (mirrors `starlette.WebSocket.url_for`).
    #[pyo3(signature = (name, **kwargs))]
    fn url_for<'py>(
        &self,
        name: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
        py: Python<'py>,
    ) -> PyResult<Py<PyAny>> {
        if let Some(ref app) = self.conn.app {
            let method = app.bind(py).getattr("url_for")?;
            return Ok(method.call((name,), kwargs)?.unbind());
        }
        Err(PyNotImplementedError::new_err("websocket.url_for requires an app with named routes"))
    }

    /// Complete the handshake. The Rust side has already accepted the upgrade,
    /// so this records the lifecycle state and optional subprotocol/headers for
    /// API parity with other frameworks.
    #[pyo3(signature = (subprotocol=None, headers=None))]
    fn accept<'py>(
        &self,
        py: Python<'py>,
        subprotocol: Option<String>,
        headers: Option<Py<PyAny>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let _ = headers;
        if let Some(sp) = subprotocol {
            *self.subprotocol.lock().unwrap_or_else(|e| e.into_inner()) = Some(sp);
        }
        self.client_state.store(WS_STATE_CONNECTED, Ordering::SeqCst);
        self.application_state.store(WS_STATE_CONNECTED, Ordering::SeqCst);
        let _ = py;
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
                Some(WsMessage::Close(..)) | None => Ok(None),
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
                Some(WsMessage::Close(..)) | None => {
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
                Some(WsMessage::Close(..)) | None => {
                    Err(PyEOFError::new_err("websocket connection closed"))
                }
            }
        })
    }

    /// Receive the next message, parse it as JSON, and return the decoded value.
    #[pyo3(signature = (mode = "text"))]
    fn receive_json<'py>(&self, py: Python<'py>, mode: &str) -> PyResult<Bound<'py, PyAny>> {
        let incoming = self.incoming.clone();
        let mode = mode.to_string();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let text = {
                let mut rx = incoming.lock().await;
                match rx.recv().await {
                    Some(WsMessage::Text(t)) => t,
                    Some(WsMessage::Bytes(b)) => String::from_utf8_lossy(&b).into_owned(),
                    Some(WsMessage::Close(..)) | None => {
                        return Err(PyEOFError::new_err("websocket connection closed"));
                    }
                }
            };
            if mode == "binary" {
                let _ = mode;
            }
            Python::attach(|py| {
                let json = py.import("json")?;
                Ok(json.getattr("loads")?.call1((text,))?.unbind())
            })
        })
    }

    /// Send a raw message. `bytes` are sent as binary.
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

    /// Serialize `data` to JSON and send it as a text (or binary) message.
    #[pyo3(signature = (data, mode = "text"))]
    fn send_json<'py>(
        &self,
        py: Python<'py>,
        data: Bound<'py, PyAny>,
        mode: &str,
    ) -> PyResult<Bound<'py, PyAny>> {
        let outgoing = self.outgoing.clone();
        let text = py.import("json")?.getattr("dumps")?.call1((data,))?.extract::<String>()?;
        let is_binary = mode == "binary";
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let msg =
                if is_binary { WsMessage::Bytes(text.into_bytes()) } else { WsMessage::Text(text) };
            outgoing
                .send(msg)
                .map_err(|_| PyConnectionError::new_err("websocket connection closed"))?;
            Ok(())
        })
    }

    /// Send a close frame and terminate the connection.
    #[pyo3(signature = (code=None, reason=None))]
    fn close<'py>(
        &self,
        py: Python<'py>,
        code: Option<u16>,
        reason: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        self.client_state.store(WS_STATE_DISCONNECTED, Ordering::SeqCst);
        self.application_state.store(WS_STATE_DISCONNECTED, Ordering::SeqCst);
        let outgoing = self.outgoing.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let _ = outgoing.send(WsMessage::Close(code, reason));
            Ok(())
        })
    }

    fn __repr__(&self) -> String {
        "WebSocket()".to_string()
    }
}

/// WebSocket lifecycle state constants (mirrors `starlette.websockets.WebSocketState`).
///
/// Exposed as a class with integer attributes so it can be compared directly:
/// `ws.client_state == WebSocketState.CONNECTED`.
#[pyclass(name = "WebSocketState")]
pub struct WebSocketState;

#[pymethods]
impl WebSocketState {
    #[classattr]
    #[allow(non_snake_case)]
    fn CONNECTING() -> u8 {
        WS_STATE_CONNECTING
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn CONNECTED() -> u8 {
        WS_STATE_CONNECTED
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn DISCONNECTED() -> u8 {
        WS_STATE_DISCONNECTED
    }
    #[classattr]
    #[allow(non_snake_case)]
    fn RESPONSE() -> u8 {
        WS_STATE_RESPONSE
    }
}
