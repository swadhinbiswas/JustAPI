use std::sync::Arc;
use hyper::Method;

use pyo3::prelude::*;
use tokio_util::sync::CancellationToken;

use crate::websocket::{WebSocket, WsMessage};
use futures::{SinkExt, StreamExt};

use justapi_core::router::Router;

use crate::database::Database;

use super::types::*;
use super::handlers::*;

#[pyclass(name = "_JustAPIApp")]
pub struct JustAPIApp {
    pub router: Router<usize>,
    pub handlers: Vec<Py<PyAny>>,
    pub schemas: Vec<Option<Py<PyAny>>>,
    pub schema_jsons: Vec<Option<String>>,
    pub batch_configs: Vec<Option<(usize, u64)>>,
    pub plugins: Vec<Py<PyAny>>,
    pub database: Option<Database>,
    pub grpc_addr: Option<std::net::SocketAddr>,
    pub grpc_handlers: std::collections::HashMap<String, Py<PyAny>>,
    pub ws_routes: std::collections::HashMap<String, Py<PyAny>>,
    pub wasm_bytes: Option<Vec<u8>>,
    pub gateway_config: Option<String>,
    pub circuit_breaker_config: Option<(usize, u64)>,
    pub coalesce_headers: Option<Vec<String>>,
}

#[pymethods]
impl JustAPIApp {
    #[new]
    fn new() -> Self {
        Self {
            router: Router::new(),
            handlers: Vec::new(),
            schemas: Vec::new(),
            schema_jsons: Vec::new(),
            batch_configs: Vec::new(),
            plugins: Vec::new(),
            database: None,
            grpc_addr: None,
            grpc_handlers: std::collections::HashMap::new(),
            ws_routes: std::collections::HashMap::new(),
            wasm_bytes: None,
            gateway_config: None,
            circuit_breaker_config: None,
            coalesce_headers: None,
        }
    }

    #[pyo3(name = "enable_gateway")]
    fn enable_gateway(&mut self, path: &str) {
        self.gateway_config = Some(path.to_string());
    }

    #[pyo3(name = "enable_circuit_breaker")]
    fn enable_circuit_breaker(&mut self, failure_threshold: usize, reset_timeout_ms: u64) {
        self.circuit_breaker_config = Some((failure_threshold, reset_timeout_ms));
    }

    /// Enable request coalescing (singleflight). When many concurrent, identical
    /// requests hit the same route, only one reaches the handler; the rest share
    /// its response. `headers`, if given, are included in the coalesce key
    /// (e.g. `["accept"]` so distinct representations are not collapsed).
    #[pyo3(name = "enable_request_coalescing")]
    fn enable_request_coalescing(&mut self, headers: Option<Vec<String>>) {
        self.coalesce_headers = Some(headers.unwrap_or_default());
    }

    fn load_wasm_middleware(&mut self, path: &str) -> PyResult<()> {
        let bytes = std::fs::read(path).map_err(|e| {
            pyo3::exceptions::PyIOError::new_err(format!(
                "Failed to read WASM file {}: {}",
                path, e
            ))
        })?;
        self.wasm_bytes = Some(bytes);
        Ok(())
    }

    /// Register a plugin. This immediately calls `plugin.build(self)`.
    #[pyo3(name = "use")]
    fn use_plugin(slf: Py<Self>, py: Python<'_>, plugin: Py<PyAny>) -> PyResult<()> {
        plugin.call_method1(py, "build", (slf.clone_ref(py),))?;
        slf.borrow_mut(py).plugins.push(plugin);
        Ok(())
    }

    fn get(&mut self, path: &str, handler: Py<PyAny>) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.batch_configs.push(None);
        self.router
            .insert(Method::GET, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, batch_size=None, batch_window_ms=None))]
    fn post(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(body_schema);
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.batch_configs
            .push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.router
            .insert(Method::POST, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, batch_size=None, batch_window_ms=None))]
    fn put(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(body_schema);
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.batch_configs
            .push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.router
            .insert(Method::PUT, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, batch_size=None, batch_window_ms=None))]
    fn patch(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(body_schema);
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.batch_configs
            .push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.router
            .insert(Method::PATCH, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    fn delete(&mut self, path: &str, handler: Py<PyAny>) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.batch_configs.push(None);
        self.router
            .insert(Method::DELETE, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Register a route for the HTTP QUERY method (RFC 10008): safe,
    /// idempotent, and body-carrying (like POST). The `experimental` flag
    /// is reserved for tagging the operation in generated OpenAPI specs.
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, experimental=None))]
    fn query(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        experimental: Option<bool>,
    ) -> PyResult<()> {
        let _ = experimental;
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.schemas.push(body_schema);
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.batch_configs.push(None);
        self.router
            .insert(justapi_core::query_method(), path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Set the database configuration.
    #[pyo3(signature = (db))]
    fn set_database(&mut self, _py: Python<'_>, db: &Bound<'_, PyAny>) -> PyResult<()> {
        if let Ok(db_obj) = db.extract::<Database>() {
            self.database = Some(db_obj);
            Ok(())
        } else if let Ok(url) = db.extract::<String>() {
            self.database = Some(Database::new(url, 10));
            Ok(())
        } else {
            Err(pyo3::exceptions::PyTypeError::new_err(
                "database must be a Database object or a connection string",
            ))
        }
    }

    /// Get the database configuration, or None.
    fn get_database(&self) -> Option<Database> {
        self.database.clone()
    }

    fn set_grpc_addr(&mut self, addr: &str) -> PyResult<()> {
        self.grpc_addr = Some(addr.parse().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid gRPC address: {}", e))
        })?);
        Ok(())
    }

    fn add_grpc_service(&mut self, path: String, handler: Py<PyAny>) {
        self.grpc_handlers.insert(path, handler);
    }

    /// Register a WebSocket handler for `path`. The handler is an async Python
    /// function receiving a single `WebSocket` argument.
    fn websocket(&mut self, path: String, handler: Py<PyAny>) -> PyResult<()> {
        if self.ws_routes.contains_key(&path) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "WebSocket route already registered: {}",
                path
            )));
        }
        self.ws_routes.insert(path, handler);
        Ok(())
    }

    /// Start the server and begin accepting requests.
    fn run(&mut self, py: Python<'_>, addr: &str) -> PyResult<()> {
        let addr: std::net::SocketAddr = addr.parse().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {}", e))
        })?;

        // Build OpenAPI spec from registered routes before taking ownership.
        let openapi_spec = {
            use justapi_core::openapi::*;
            let routes: Vec<(hyper::Method, String)> = self.router.list_routes().to_vec();
            let mut registry = OpenApiRegistry::new();
            for (method, path) in &routes {
                let _path_params: Vec<String> = path
                    .split('/')
                    .filter(|s| s.starts_with('{') && s.ends_with('}'))
                    .map(|s| s[1..s.len() - 1].to_string())
                    .collect();
                let is_body_method = matches!(
                    *method,
                    hyper::Method::POST | hyper::Method::PUT | hyper::Method::PATCH
                );
                registry.register(RouteMeta {
                    method: method.clone(),
                    path: path.clone(),
                    summary: None,
                    description: None,
                    tags: vec![],
                    request_body_schema: if is_body_method {
                        Some(serde_json::json!({}))
                    } else {
                        None
                    },
                    response_schema: Some(serde_json::json!({})),
                    deprecated: false,
                    experimental: false,
                });
            }
            if routes.is_empty() {
                None
            } else {
                Some(
                    serde_json::to_string_pretty(&registry.generate("JustAPI", "2.0.0"))
                        .unwrap_or_default(),
                )
            }
        };

        let router = Arc::new(std::mem::take(&mut self.router));
        let handlers = Arc::new(std::mem::take(&mut self.handlers));
        let schemas = Arc::new(std::mem::take(&mut self.schemas));
        let schema_jsons = Arc::new(std::mem::take(&mut self.schema_jsons));
        let batch_configs_arc = Arc::new(std::mem::take(&mut self.batch_configs));
        let database_config = self.database.take().map(|d| d.to_config());
        let plugins = std::mem::take(&mut self.plugins);
        let grpc_addr = self.grpc_addr.take();
        let grpc_handlers = Arc::new(std::mem::take(&mut self.grpc_handlers));
        let ws_routes = std::mem::take(&mut self.ws_routes);
        let wasm_bytes = self.wasm_bytes.take();
        let gateway_config_path = self.gateway_config.take();
        let cb_config = self.circuit_breaker_config.take();
        let coalesce_headers = self.coalesce_headers.take();

        let shutdown = CancellationToken::new();
        let shutdown_signal = shutdown.clone();

        for plugin in &plugins {
            call_plugin_hook(py, plugin, "on_startup")?;
        }

        // `py.detach` blocks the calling thread while releasing the GIL
        // (standard server-entrypoint behavior, like `uvicorn.run`), keeping
        // the process alive until shutdown. Handler threads call back into
        // Python via `Python::attach`.
        let result = py.detach(move || -> Result<(), anyhow::Error> {
            let rt = tokio::runtime::Runtime::new()?;
            rt.block_on(async {
                // Initialize database pool if configured, and build transaction-aware handler.
                let db_pool = if let Some(ref config) = database_config {
                    let mut mgr = justapi_core::db::PoolManager::new();
                    match mgr.init("", config.clone()).await {
                        Ok(pool) => {
                            tracing::info!("Database pool initialized ({:?})", pool.kind());
                            Some(pool)
                        }
                        Err(e) => {
                            tracing::error!("Failed to initialize database pool: {}", e);
                            return Err(anyhow::anyhow!("Database init error: {}", e));
                        }
                    }
                } else {
                    None
                };

                let db_url_str = database_config.as_ref().map(|c| c.url.clone());

                let mut batchers = Vec::new();
                for (id, config) in batch_configs_arc.iter().enumerate() {
                    if let Some((max_size, window_ms)) = config {
                        let handlers_arc = handlers.clone();
                        let db_url_str2 = db_url_str.clone();
                        let batcher = justapi_core::batching::start_batcher(
                            *max_size,
                            std::time::Duration::from_millis(*window_ms),
                            move |batch: Vec<BatchedReq>| {
                                let handlers_arc = handlers_arc.clone();
                                let db_url_str2 = db_url_str2.clone();
                                async move {
                                    tokio::task::spawn_blocking(move || {
                                        Python::attach(|py| {
                                            let py_handler = handlers_arc[id].clone_ref(py);
                                            let helper = get_helper(py);
                                            let py_list = pyo3::types::PyList::empty(py);
                                            for req in &batch {
                                                let r = Bound::new(py, crate::request::Request::new(
                                                    py,
                                                    req.method.clone(),
                                                    req.path.clone(),
                                                    req.path_params.to_vec(),
                                                    req.query_string.to_vec(),
                                                    req.headers.to_vec(),
                                                    req.body.to_vec(),
                                                    db_url_str2.as_deref().map(|s| s.to_string()),
                                                    None,
                                                )).unwrap();
                                                py_list.append(r).unwrap();
                                            }

                                            let result = helper.call_batch_handler.bind(py).call1((py_handler.bind(py), py_list));
                                            let final_res = match result {
                                                Ok(res) => {
                                                    let is_future = res.hasattr("result").unwrap_or(false) && !res.is_instance_of::<pyo3::types::PyList>();
                                                    if is_future {
                                                        res.call_method0("result")
                                                    } else {
                                                        Ok(res)
                                                    }
                                                }
                                                Err(e) => Err(e),
                                            };

                                            let mut results = Vec::new();
                                            match final_res {
                                                Ok(res_list) => {
                                                    if let Ok(list) = res_list.extract::<Vec<pyo3::Bound<'_, pyo3::PyAny>>>() {
                                                        for item in list.iter() {
                                                            let status: u16 = item.get_item("status").ok().and_then(|v| v.extract().ok()).unwrap_or(200);
                                                            let headers: Vec<(Vec<u8>, Vec<u8>)> = item.get_item("headers").ok().and_then(|v| v.extract().ok()).unwrap_or_default();
                                                            let body: Vec<u8> = item.get_item("body").ok().and_then(|v| v.extract().ok()).unwrap_or_default();
                                                            results.push(NativeResponse { status, headers, body: NativeBody::Bytes(body) });
                                                        }
                                                    }
                                                }
                                                Err(e) => {
                                                    tracing::error!("Batch handler error: {}", e);
                                                    for _ in 0..batch.len() {
                                                        results.push(NativeResponse {
                                                            status: 500,
                                                            headers: vec![],
                                                            body: NativeBody::Bytes(b"Internal Server Error".to_vec()),
                                                        });
                                                    }
                                                }
                                            }

                                            // Handle case where user returns wrong number of elements
                                            while results.len() < batch.len() {
                                                results.push(NativeResponse {
                                                    status: 500,
                                                    headers: vec![],
                                                    body: NativeBody::Bytes(b"Batch length mismatch".to_vec()),
                                                });
                                            }
                                            results.truncate(batch.len());

                                            results
                                        })
                                    }).await.unwrap_or_default()
                                }
                            }
                        );
                        batchers.push(Some(batcher));
                    } else {
                        batchers.push(None);
                    }
                }
                let batchers = Arc::new(batchers);
                let handler = make_native_handler(
                    router,
                    handlers,
                    schemas,
                    schema_jsons,
                    batchers,
                    db_pool.clone(),
                    db_url_str,
                );

                // Install Ctrl+C signal handler to trigger graceful shutdown.
                let signal_token = shutdown.clone();
                tokio::spawn(async move {
                    tokio::signal::ctrl_c().await.ok();
                    tracing::info!("Ctrl+C received, initiating graceful shutdown");
                    signal_token.cancel();
                });

                let mut server = justapi_core::Server::new(addr)
                    .with_handler(handler)
                    .with_shutdown(shutdown_signal);
                if let Some(ref spec) = openapi_spec {
                    server = server.with_openapi_spec(spec.clone());
                }

                if let Some(ref wasm_bytes) = wasm_bytes {
                    server = match server.with_wasm_middleware(wasm_bytes) {
                        Ok(s) => s,
                        Err(e) => {
                            tracing::error!("Failed to initialize WASM middleware: {}", e);
                            return Err(anyhow::anyhow!("WASM middleware error: {}", e));
                        }
                    };
                }

                if let Some((failure_threshold, reset_timeout_ms)) = cb_config {
                    tracing::info!("Enabling Global Circuit Breaker (threshold: {}, reset: {}ms)", failure_threshold, reset_timeout_ms);
                    let config = justapi_core::resilience::CircuitBreakerConfig {
                        failure_threshold: failure_threshold as u32,
                        open_timeout: std::time::Duration::from_millis(reset_timeout_ms),
                        ..Default::default()
                    };
                    server = server.add_circuit_breaker(config);
                }

                if let Some(headers) = coalesce_headers {
                    let mut coalescer = justapi_core::coalesce::RequestCoalescer::new();
                    if !headers.is_empty() {
                        let mut names = Vec::new();
                        for h in &headers {
                            match hyper::header::HeaderName::from_bytes(h.as_bytes()) {
                                Ok(n) => names.push(n),
                                Err(e) => {
                                    tracing::error!(
                                        "Invalid coalesce header {} ignored: {}",
                                        h,
                                        e
                                    );
                                }
                            }
                        }
                        coalescer = coalescer.with_headers(&names);
                    }
                    tracing::info!(
                        "Enabling request coalescing (singleflight){}",
                        if headers.is_empty() {
                            String::new()
                        } else {
                            format!(" with key headers: {:?}", headers)
                        }
                    );
                    server = server.add_middleware(coalescer);
                }

                if let Some(path) = gateway_config_path {
                    tracing::info!("Starting Gateway with hot reloading for config: {}", path);
                    let gateway_state = justapi_core::gateway::GatewayState::new(&path);
                    if let Err(e) = gateway_state.clone().watch() {
                        tracing::error!("Failed to start gateway config file watcher: {}", e);
                    }
                    server = server.add_gateway(gateway_state);
                }

                if let (Some(gaddr), true) = (grpc_addr, !grpc_handlers.is_empty()) {
                    let g_handlers = grpc_handlers.clone();
                    let grpc_h = Box::new(move |uri: http::Uri, body: Vec<u8>| {
                        let g_handlers = g_handlers.clone();
                        Box::pin(async move {
                            let path = uri.path().to_string();
                            let (tx, rx) = tokio::sync::oneshot::channel();
                            tokio::task::spawn_blocking(move || {
                                Python::attach(|py| {
                                    if let Some(h) = g_handlers.get(&path) {
                                        let py_body = pyo3::types::PyBytes::new(py, &body);
                                        match h.call1(py, (py_body,)) {
                                            Ok(res) => {
                                                if let Ok(b) = res.extract::<Vec<u8>>(py) {
                                                    let _ = tx.send(Ok(b));
                                                } else {
                                                    let _ = tx.send(Err(tonic::Status::internal("Invalid return type from python gRPC handler")));
                                                }
                                            }
                                            Err(e) => {
                                                let err_msg = e.to_string();
                                                let _ = tx.send(Err(tonic::Status::internal(err_msg)));
                                            }
                                        }
                                    } else {
                                        let _ = tx.send(Err(tonic::Status::unimplemented("Method not found")));
                                    }
                                });
                            });
                            rx.await.unwrap_or_else(|_| Err(tonic::Status::internal("Channel dropped")))
                        }) as std::pin::Pin<Box<dyn std::future::Future<Output = Result<Vec<u8>, tonic::Status>> + Send + 'static>>
                    });
                    server = server.with_grpc(gaddr, grpc_h);
                }

                // WebSocket handlers: build a WsHandler that bridges the
                // accepted tokio-tungstenite stream to a Python handler.
                if !ws_routes.is_empty() {
                    let ws_routes_arc = Arc::new(ws_routes);
                    let ws_handler: justapi_core::server::WsHandler = Arc::new(move |path, mut read, mut write| {
                        let ws_routes = ws_routes_arc.clone();
                        Box::pin(async move {
                            let (out_tx, mut out_rx) =
                                tokio::sync::mpsc::unbounded_channel::<WsMessage>();
                            let (in_tx, in_rx) = tokio::sync::mpsc::unbounded_channel::<WsMessage>();
                            let incoming =
                                std::sync::Arc::new(tokio::sync::Mutex::new(in_rx));

                            // Inbound WebSocket messages -> Python incoming channel.
                            tokio::spawn(async move {
                                while let Some(msg) = read.next().await {
                                    match msg {
                                        Ok(tokio_tungstenite::tungstenite::Message::Text(t)) => {
                                            let _ = in_tx
                                                .send(WsMessage::Text(t.to_string()));
                                        }
                                        Ok(tokio_tungstenite::tungstenite::Message::Binary(b)) => {
                                            let _ = in_tx.send(WsMessage::Bytes(b.to_vec()));
                                        }
                                        Ok(tokio_tungstenite::tungstenite::Message::Close(_))
                                        | Err(_) => {
                                            let _ = in_tx.send(WsMessage::Close);
                                            break;
                                        }
                                        _ => {}
                                    }
                                }
                            });

                            // Python outgoing channel -> WebSocket sink.
                            tokio::spawn(async move {
                                while let Some(msg) = out_rx.recv().await {
                                    let frame = match &msg {
                                        WsMessage::Text(t) => {
                                            tokio_tungstenite::tungstenite::Message::Text(t.into())
                                        }
                                        WsMessage::Bytes(b) => {
                                            tokio_tungstenite::tungstenite::Message::Binary(
                                                b.clone(),
                                            )
                                        }
                                        WsMessage::Close => {
                                            tokio_tungstenite::tungstenite::Message::Close(None)
                                        }
                                    };
                                    if write.send(frame).await.is_err() {
                                        break;
                                    }
                                    if matches!(msg, WsMessage::Close) {
                                        break;
                                    }
                                }
                                let _ = write.close().await;
                            });

                            // Invoke the Python handler on the daemon event loop.
                            Python::attach(|py| {
                                if let Some(h) = ws_routes.get(&path) {
                                    match h
                                        .bind(py)
                                        .call1((WebSocket::new(incoming, out_tx),))
                                    {
                                        Ok(coro) => {
                                            let helper = get_helper(py);
                                            if let Err(e) =
                                                helper.run_ws_handler.bind(py).call1((coro,))
                                            {
                                                tracing::error!(
                                                    "Failed to schedule WebSocket handler: {}",
                                                    e
                                                );
                                            }
                                        }
                                        Err(e) => {
                                            tracing::error!("WebSocket handler call error: {}", e)
                                        }
                                    }
                                }
                            });

                            Ok(())
                        })
                    });
                    server = server.with_ws(ws_handler);
                }

                server.run().await
            })
        });

        for plugin in &plugins {
            if let Err(e) = call_plugin_hook(py, plugin, "on_shutdown") {
                tracing::error!("Error in plugin on_shutdown: {}", e);
            }
        }

        result
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Server error: {}", e)))
    }
}
