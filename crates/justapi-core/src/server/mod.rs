use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::LazyLock;

use anyhow::Result;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::service::service_fn;
use hyper::{Method, Request, Response, StatusCode};
use hyper_util::rt::TokioIo;
use tokio::net::TcpListener;

#[cfg(feature = "compression")]
use crate::compress::CompressionMiddleware;
use crate::health::HealthRegistry;
use crate::memory::{BufferPool, SharedArena};
use crate::metrics::{self, Metrics};
use crate::middleware::{
    ApiKeyAuth, Cors, JwtAuth, MiddlewareChain, OAuth2Password, RateLimiter, SecurityHeaders,
};
use crate::openapi;
use crate::plugin::PluginRegistry;
use crate::router::{Match, Router};
use crate::static_files::StaticDir;
use crate::{json_response, streaming_response, ResponseBody};
#[cfg(feature = "ws")]
use futures::StreamExt;
use serde_json;
use tokio_util::sync::CancellationToken;

// ---------------------------------------------------------------------------
// WebSocket handler (feature-gated on `ws`)
// ---------------------------------------------------------------------------

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

/// A handler invoked when a WebSocket upgrade is accepted on a registered path.
/// Receives the request path plus the split read/write halves of the accepted
/// WebSocket stream. The handler owns the connection for its lifetime.
#[cfg(feature = "ws")]
pub type WsHandler = std::sync::Arc<
    dyn Fn(
            String,
            WsRead,
            WsWrite,
        )
            -> std::pin::Pin<Box<dyn std::future::Future<Output = anyhow::Result<()>> + Send>>
        + Send
        + Sync,
>;

#[cfg(feature = "ws")]
use tokio_tungstenite::tungstenite::Message;

/// Default WebSocket handler used when no application handler is registered.
/// Echoes text/binary frames (mirroring the legacy raw-TCP behavior) so that
/// standalone servers remain WebSocket-compatible out of the box. `with_ws`
/// replaces this with an application-provided handler.
#[cfg(feature = "ws")]
fn default_ws_echo() -> WsHandler {
    std::sync::Arc::new(|_path, mut read, mut write| {
        Box::pin(async move {
            use futures::{SinkExt, StreamExt};
            while let Some(msg) = read.next().await {
                match msg {
                    Ok(m @ Message::Text(_)) | Ok(m @ Message::Binary(_)) => {
                        if write.send(m).await.is_err() {
                            break;
                        }
                    }
                    Ok(Message::Ping(p)) => {
                        let _ = write.send(Message::Pong(p)).await;
                    }
                    Ok(Message::Close(_)) | Err(_) => {
                        let _ = write.send(Message::Close(None)).await;
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

// ---------------------------------------------------------------------------
// Handler enum
// ---------------------------------------------------------------------------

#[derive(Clone)]
#[allow(dead_code)]
pub(crate) enum Handler {
    Static { status: StatusCode, body: &'static str },
    Echo,
    ParamsEcho,
    Sse,
    Health,
    Ready,
    Live,
    Prometheus,
    OpenApiJson,
    SwaggerUi,
    Redoc,
    GraphQL,
    Custom(crate::middleware::HandlerFn),
}

static BUILTIN_SPEC: LazyLock<String> = LazyLock::new(|| {
    let spec = serde_json::json!({
        "openapi": "3.1.0",
        "info": {
            "title": "JustAPI",
            "version": "0.1.0",
            "description": "JustAPI Runtime — Python application server"
        },
        "servers": [{"url": "http://localhost:8080", "description": "Local development"}],
        "tags": [{"name": "builtin", "description": "Built-in routes"}],
        "paths": {
            "/hello": {
                "get": {
                    "summary": "Hello world",
                    "description": "Returns a greeting message",
                    "operationId": "get_hello",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "Greeting"}}
                }
            },
            "/echo": {
                "post": {
                    "summary": "Echo request body",
                    "description": "Returns the request body as-is",
                    "operationId": "post_echo",
                    "tags": ["builtin"],
                    "requestBody": {
                        "description": "Any JSON payload",
                        "content": {"application/json": {"schema": {}}},
                        "required": true
                    },
                    "responses": {"200": {"description": "Echoed body"}}
                }
            },
            "/users/{id}": {
                "get": {
                    "summary": "Get user by ID",
                    "operationId": "get_users_id",
                    "tags": ["builtin"],
                    "parameters": [{
                        "name": "id",
                        "in": "path",
                        "description": "User ID",
                        "required": true,
                        "schema": {"type": "string"}
                    }],
                    "responses": {
                        "200": {"description": "User found"},
                        "404": {"description": "Not found"}
                    }
                }
            },
            "/events": {
                "get": {
                    "summary": "Server-Sent Events stream",
                    "description": "Streams count events via SSE",
                    "operationId": "get_events",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "SSE event stream"}}
                }
            },
            "/health": {
                "get": {
                    "summary": "Health check",
                    "description": "Returns health status of all registered components",
                    "operationId": "get_health",
                    "tags": ["builtin"],
                    "responses": {
                        "200": {"description": "Healthy"},
                        "503": {"description": "Unhealthy"}
                    }
                }
            },
            "/ready": {
                "get": {
                    "summary": "Readiness probe",
                    "description": "Returns readiness status for Kubernetes probes",
                    "operationId": "get_ready",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "Ready"}}
                }
            },
            "/live": {
                "get": {
                    "summary": "Liveness probe",
                    "description": "Returns liveness status for Kubernetes probes",
                    "operationId": "get_live",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "Alive"}}
                }
            },
            "/metrics": {
                "get": {
                    "summary": "Prometheus metrics",
                    "description": "Returns Prometheus-formatted metrics",
                    "operationId": "get_metrics",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "Metrics in Prometheus text format"}}
                }
            },
            "/openapi.json": {
                "get": {
                    "summary": "OpenAPI 3.1 spec",
                    "description": "Returns the OpenAPI 3.1 specification for this server",
                    "operationId": "get_openapi_json",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "OpenAPI spec"}}
                }
            },
            "/docs": {
                "get": {
                    "summary": "Swagger UI",
                    "description": "Interactive API documentation via Swagger UI",
                    "operationId": "get_docs",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "Swagger UI HTML"}}
                }
            },
            "/redoc": {
                "get": {
                    "summary": "ReDoc",
                    "description": "Interactive API documentation via ReDoc",
                    "operationId": "get_redoc",
                    "tags": ["builtin"],
                    "responses": {"200": {"description": "ReDoc HTML"}}
                }
            }
        }
    });
    serde_json::to_string_pretty(&spec).unwrap()
});

// ---------------------------------------------------------------------------
// TLS config
// ---------------------------------------------------------------------------

#[cfg(feature = "tls")]
pub struct TlsConfig {
    pub cert_path: String,
    pub key_path: String,
}

// ---------------------------------------------------------------------------
// Server builder
// ---------------------------------------------------------------------------

pub struct Server {
    addr: SocketAddr,
    chain: MiddlewareChain,
    static_dir: Option<StaticDir>,
    metrics: Metrics,
    health_registry: Arc<HealthRegistry>,
    #[cfg(feature = "tls")]
    tls: Option<TlsConfig>,
    shutdown: Option<CancellationToken>,
    plugin_registry: PluginRegistry,
    wasm_middleware: Option<Arc<crate::wasm::WasmEngine>>,
    grpc_addr: Option<SocketAddr>,
    grpc_handler: Option<crate::grpc::GrpcHandler>,
    #[cfg(feature = "ws")]
    ws_handler: Option<WsHandler>,
    router: Option<Router<Handler>>,
    /// Tracks whether a custom handler (via `with_handler`) was installed, so we
    /// can refuse to also install the default router (which would silently
    /// discard the custom handler — see `with_default_routes`).
    custom_handler_set: bool,
    /// Tracks whether the default router (via `with_default_routes`) was
    /// installed, so we can refuse a later `with_handler` (same footgun).
    default_routes_set: bool,
    openapi_spec: Option<Arc<String>>,
}

impl Server {
    pub fn new(addr: SocketAddr) -> Self {
        let router = Router::new();
        let metrics = Metrics::new();
        let health_registry = Arc::new(HealthRegistry::new());
        // We initialize chain with a dummy handler. The real one is set in run().
        let chain = MiddlewareChain::new(Arc::new(|_| {
            Box::pin(async {
                Ok(Response::builder()
                    .status(StatusCode::NOT_FOUND)
                    .body(crate::UnsyncBoxBody::new(
                        http_body_util::Full::new(Bytes::from("Not Found"))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                    ))
                    .unwrap())
            })
        }));
        Self {
            addr,
            chain,
            static_dir: None,
            metrics,
            health_registry,
            shutdown: None,
            plugin_registry: PluginRegistry::new(),
            wasm_middleware: None,
            #[cfg(feature = "tls")]
            tls: None,
            grpc_addr: None,
            grpc_handler: None,
            #[cfg(feature = "ws")]
            ws_handler: Some(default_ws_echo()),
            router: Some(router),
            custom_handler_set: false,
            default_routes_set: false,
            openapi_spec: None,
        }
    }

    /// Replace the built-in OpenAPI spec with a dynamically generated one
    /// (e.g., from a Python JustAPIApp's registered routes).
    pub fn with_openapi_spec(mut self, spec: String) -> Self {
        self.openapi_spec = Some(Arc::new(spec));
        self
    }

    /// Register a WebSocket handler. When a WebSocket upgrade is accepted on any
    /// path, the handler is invoked with the path and the split stream halves.
    #[cfg(feature = "ws")]
    pub fn with_ws(mut self, handler: WsHandler) -> Self {
        self.ws_handler = Some(handler);
        self
    }

    /// Set a custom fallback handler (used by the ASGI shim).
    pub fn with_handler(mut self, handler: crate::middleware::HandlerFn) -> Self {
        // A custom handler and the built-in default router are mutually
        // exclusive: installing the default router after a custom handler would
        // silently discard every user route (the router-derived handler wins in
        // `run`). Fail loudly instead of losing routes without explanation.
        if self.default_routes_set {
            panic!(
                "with_handler() called after with_default_routes(): the default router would \
                 silently discard your custom handler. Choose one path — custom handler OR \
                 default routes — not both."
            );
        }
        // If the user specifies a custom handler, it replaces the entire router behavior.
        self.router = None;
        self.chain.set_handler(handler);
        self.custom_handler_set = true;
        self
    }

    pub fn with_default_routes(mut self) -> Self {
        if self.custom_handler_set {
            panic!(
                "with_default_routes() called after with_handler(): it would silently discard \
                 your custom handler and every user route. Choose one path — default routes OR \
                 a custom handler — not both."
            );
        }
        let mut router = self.router.unwrap_or_default();
        router
            .insert(
                Method::GET,
                "/hello",
                Handler::Static { status: StatusCode::OK, body: r#"{"message":"hello"}"# },
            )
            .expect("valid");
        router.insert(Method::POST, "/echo", Handler::Echo).expect("valid");
        router.insert(Method::GET, "/echo/{*rest}", Handler::ParamsEcho).expect("valid");
        router
            .insert(
                Method::GET,
                "/users/{id}",
                Handler::Static { status: StatusCode::OK, body: r#"{"message":"user lookup"}"# },
            )
            .expect("valid");
        router.insert(Method::GET, "/events", Handler::Sse).expect("valid");
        router.insert(Method::GET, "/graphql", Handler::GraphQL).expect("valid");
        router.insert(Method::POST, "/graphql", Handler::GraphQL).expect("valid");
        router.set_fallback(Handler::Static {
            status: StatusCode::NOT_FOUND,
            body: r#"{"error":"not found"}"#,
        });
        self.router = Some(router);
        self.default_routes_set = true;
        self
    }

    /// Register the OpenAI-compatible inference endpoints, bound to `engine`.
    ///
    /// Routes added:
    /// - `POST /v1/chat/completions`
    /// - `POST /v1/completions`
    /// - `POST /v1/embeddings`
    /// - `GET  /v1/models`
    #[cfg(feature = "inference")]
    pub fn with_openai(mut self, engine: std::sync::Arc<justapi_inference::Engine>) -> Self {
        if let Some(ref mut r) = self.router {
            r.insert(
                hyper::Method::POST,
                "/v1/chat/completions",
                Handler::Custom(crate::openai::chat_completions_handler(engine.clone())),
            )
            .expect("valid route: /v1/chat/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/completions",
                Handler::Custom(crate::openai::completions_handler(engine.clone())),
            )
            .expect("valid route: /v1/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/embeddings",
                Handler::Custom(crate::openai::embeddings_handler(engine.clone())),
            )
            .expect("valid route: /v1/embeddings");
            r.insert(
                hyper::Method::GET,
                "/v1/models",
                Handler::Custom(crate::openai::models_handler(engine.clone())),
            )
            .expect("valid route: /v1/models");
        }
        self
    }

    /// Register the OpenAI-compatible inference endpoints backed by the
    /// scheduler (continuous batching + RadixAttention prefix cache).
    ///
    /// Same routes as [`Server::with_openai`], but each request is admitted
    /// through the scheduler's wait queue, scheduled for prefill/decode steps,
    /// and prefix-cached when finished.  Scheduler prefix-cache statistics are
    /// exposed via the `/metrics` endpoint.
    ///
    /// The [`SchedulerEngine`](justapi_inference::SchedulerEngine) wraps the
    /// engine and scheduler into a single generation interface.
    #[cfg(feature = "inference")]
    pub fn with_openai_scheduled(
        mut self,
        scheduler_engine: std::sync::Arc<justapi_inference::SchedulerEngine>,
    ) -> Self {
        // Register a metric provider that renders scheduler stats.
        let stats_provider = {
            let se = scheduler_engine.clone();
            move || {
                let s = se.stats();
                format!(
                    concat!(
                        "# HELP justapi_scheduler_waiting Current number of waiting requests.\n",
                        "# TYPE justapi_scheduler_waiting gauge\n",
                        "justapi_scheduler_waiting {}\n",
                        "# HELP justapi_scheduler_running Current number of running sequences.\n",
                        "# TYPE justapi_scheduler_running gauge\n",
                        "justapi_scheduler_running {}\n",
                        "# HELP justapi_scheduler_prefilling Current number of prefilling sequences.\n",
                        "# TYPE justapi_scheduler_prefilling gauge\n",
                        "justapi_scheduler_prefilling {}\n",
                        "# HELP justapi_scheduler_back_pressure Whether the scheduler is under back-pressure.\n",
                        "# TYPE justapi_scheduler_back_pressure gauge\n",
                        "justapi_scheduler_back_pressure {}\n",
                        "# HELP justapi_scheduler_prefix_hits Total prefix-cache hits.\n",
                        "# TYPE justapi_scheduler_prefix_hits counter\n",
                        "justapi_scheduler_prefix_hits {}\n",
                        "# HELP justapi_scheduler_prefix_misses Total prefix-cache misses.\n",
                        "# TYPE justapi_scheduler_prefix_misses counter\n",
                        "justapi_scheduler_prefix_misses {}\n",
                        "# HELP justapi_scheduler_prefix_tokens_saved Tokens of prefill saved by prefix cache.\n",
                        "# TYPE justapi_scheduler_prefix_tokens_saved counter\n",
                        "justapi_scheduler_prefix_tokens_saved {}\n",
                        "# HELP justapi_scheduler_cached_kv_blocks KV blocks resident in prefix cache.\n",
                        "# TYPE justapi_scheduler_cached_kv_blocks gauge\n",
                        "justapi_scheduler_cached_kv_blocks {}\n",
                    ),
                    s.num_waiting,
                    s.num_running,
                    s.num_prefilling,
                    s.back_pressure as u8,
                    s.prefix.hits,
                    s.prefix.misses,
                    s.prefix.tokens_saved,
                    s.cached_kv_blocks,
                )
            }
        };
        self.metrics.register_extra_provider(Box::new(stats_provider));

        if let Some(ref mut r) = self.router {
            r.insert(
                hyper::Method::POST,
                "/v1/chat/completions",
                Handler::Custom(crate::openai::scheduled_chat_completions_handler(
                    scheduler_engine.clone(),
                )),
            )
            .expect("valid route: /v1/chat/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/completions",
                Handler::Custom(crate::openai::scheduled_completions_handler(
                    scheduler_engine.clone(),
                )),
            )
            .expect("valid route: /v1/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/embeddings",
                Handler::Custom(crate::openai::embeddings_handler(
                    scheduler_engine.engine().clone(),
                )),
            )
            .expect("valid route: /v1/embeddings");
            r.insert(
                hyper::Method::GET,
                "/v1/models",
                Handler::Custom(crate::openai::models_handler(scheduler_engine.engine().clone())),
            )
            .expect("valid route: /v1/models");
        }
        self
    }

    /// Register the OpenAI-compatible inference endpoints with LoRA-aware /
    /// KV-aware routing via a [`justapi_inference::ControlPlane`] and
    /// [`justapi_inference::Router`].
    ///
    /// Each request is admitted through the control plane + router before
    /// generation: if no replica can serve it (all at capacity / no route), the
    /// endpoint returns `503`. The resolved model name is then used for the
    /// actual generation. Same routes as [`Server::with_openai`].
    #[cfg(feature = "inference")]
    pub fn with_openai_routed(
        mut self,
        engine: std::sync::Arc<justapi_inference::Engine>,
        control_plane: std::sync::Arc<justapi_inference::ControlPlane>,
        router: std::sync::Arc<justapi_inference::Router>,
    ) -> Self {
        if let Some(ref mut r) = self.router {
            r.insert(
                hyper::Method::POST,
                "/v1/chat/completions",
                Handler::Custom(crate::openai::routed_chat_completions_handler(
                    engine.clone(),
                    control_plane.clone(),
                    router.clone(),
                )),
            )
            .expect("valid route: /v1/chat/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/completions",
                Handler::Custom(crate::openai::routed_completions_handler(
                    engine.clone(),
                    control_plane.clone(),
                    router.clone(),
                )),
            )
            .expect("valid route: /v1/completions");
            r.insert(
                hyper::Method::POST,
                "/v1/embeddings",
                Handler::Custom(crate::openai::routed_embeddings_handler(
                    engine.clone(),
                    control_plane.clone(),
                    router.clone(),
                )),
            )
            .expect("valid route: /v1/embeddings");
            r.insert(
                hyper::Method::GET,
                "/v1/models",
                Handler::Custom(crate::openai::models_handler(engine.clone())),
            )
            .expect("valid route: /v1/models");
        }
        self
    }
    /// new connections and the `run()` future completes.
    pub fn with_shutdown(mut self, token: CancellationToken) -> Self {
        self.shutdown = Some(token);
        self
    }

    /// Attach a Gateway configuration with hot reloading
    pub fn add_gateway(mut self, state: std::sync::Arc<crate::gateway::GatewayState>) -> Self {
        self.chain.add(crate::gateway::GatewayMiddleware::new(state));
        self
    }

    pub fn with_static_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.static_dir = Some(StaticDir::new(dir));
        self
    }

    /// Register a plugin with the server.
    /// This calls the `build` hook immediately to allow the plugin to modify the Server configuration.
    pub fn register_plugin(mut self, plugin: Box<dyn crate::plugin::Plugin>) -> Result<Self> {
        plugin.build(&mut self)?;
        self.plugin_registry.register(plugin);
        Ok(self)
    }

    pub fn add_cors(mut self, cors: Cors) -> Self {
        self.chain.add(cors);
        self
    }

    pub fn add_security_headers(mut self, sh: SecurityHeaders) -> Self {
        self.chain.add(sh);
        self
    }

    pub fn add_rate_limiter(mut self, rl: RateLimiter) -> Self {
        self.chain.add(rl);
        self
    }

    /// Add JWT authentication middleware.
    pub fn add_jwt(mut self, jwt: JwtAuth) -> Self {
        self.chain.add(jwt);
        self
    }

    /// Add API-key authentication middleware (the "API-key scheme").
    pub fn add_api_key_auth(mut self, auth: ApiKeyAuth) -> Self {
        self.chain.add(auth);
        self
    }

    /// Register the OAuth2 password flow: token endpoint + JwtAuth middleware.
    ///
    /// This registers `{token_path}` (default `/token`) as a POST handler that
    /// accepts `application/x-www-form-urlencoded` with `grant_type=password`,
    /// `username`, and `password`. On success it returns an `access_token`
    /// signed by this provider. It also adds a [`JwtAuth`] middleware that
    /// verifies every request (except the token path) against the same key.
    ///
    /// Use [`OAuth2Password::jwt_auth`] directly if you need to customise the
    /// JWT verification rules (e.g. per-path exemptions).
    pub fn with_oauth2_password(mut self, oauth2: OAuth2Password) -> Self {
        let path = oauth2.token_path.clone();
        let handler = oauth2.token_handler::<Incoming>();
        if let Some(ref mut r) = self.router {
            r.insert(hyper::Method::POST, &path, Handler::Custom(handler))
                .expect("valid oauth2 token path");
        }
        let jwt = oauth2.jwt_auth().require_for(&path, crate::middleware::JwtRequirement::None);
        self.chain.add(jwt);
        self
    }

    pub fn add_middleware(mut self, mw: impl crate::middleware::Middleware + 'static) -> Self {
        self.chain.add(mw);
        self
    }

    /// Add response compression middleware (Gzip, feature-gated Brotli/Zstd).
    #[cfg(feature = "compression")]
    pub fn add_compression(mut self) -> Self {
        self.chain.add(CompressionMiddleware::new());
        self
    }

    pub fn add_circuit_breaker(mut self, config: crate::resilience::CircuitBreakerConfig) -> Self {
        self.chain.add(crate::resilience::PerRouteCircuitBreakerMiddleware::new(config));
        self
    }

    #[cfg(feature = "tls")]
    pub fn with_tls(mut self, config: TlsConfig) -> Self {
        self.tls = Some(config);
        self
    }

    pub fn metrics(&self) -> &Metrics {
        &self.metrics
    }

    pub fn health_registry(&self) -> &HealthRegistry {
        &self.health_registry
    }

    /// Return a cloned `Arc` handle to the health registry so other owners
    /// (e.g. the Python binding layer) can run checks independently.
    pub fn health_registry_arc(&self) -> std::sync::Arc<HealthRegistry> {
        std::sync::Arc::clone(&self.health_registry)
    }

    pub fn with_health_registry(mut self, registry: HealthRegistry) -> Self {
        self.health_registry = Arc::new(registry);
        self
    }

    pub fn with_plugin(mut self, plugin: Box<dyn crate::plugin::Plugin>) -> Self {
        self.plugin_registry.register(plugin);
        self
    }

    pub fn with_grpc(mut self, addr: SocketAddr, handler: crate::grpc::GrpcHandler) -> Self {
        self.grpc_addr = Some(addr);
        self.grpc_handler = Some(handler);
        self
    }
    pub fn with_wasm_middleware(mut self, wasm_bytes: &[u8]) -> Result<Self> {
        let engine = crate::wasm::WasmEngine::new(wasm_bytes)?;
        self.wasm_middleware = Some(Arc::new(engine));
        Ok(self)
    }

    pub async fn run(mut self) -> Result<()> {
        if let Some(router) = self.router.take() {
            let pool = Arc::new(BufferPool::new());
            let handler = make_handler(
                Arc::new(router),
                pool,
                self.metrics.clone(),
                self.health_registry.clone(),
                self.openapi_spec.clone(),
            );
            self.chain.set_handler(handler);
        }

        let listener = tokio::net::TcpListener::bind(self.addr).await?;
        let local_addr = listener.local_addr()?;

        if let (Some(grpc_addr), Some(grpc_handler)) = (self.grpc_addr, self.grpc_handler.take()) {
            tracing::info!("Starting gRPC server on {}", grpc_addr);
            let grpc_service = crate::grpc::DynamicGrpcService::new(grpc_handler);
            let shutdown = self.shutdown.clone();
            tokio::spawn(async move {
                if let Ok(grpc_listener) = tokio::net::TcpListener::bind(grpc_addr).await {
                    loop {
                        if let Some(ref token) = shutdown {
                            tokio::select! {
                                result = grpc_listener.accept() => {
                                    if let Ok((stream, _)) = result {
                                        let io = hyper_util::rt::TokioIo::new(stream);
                                        let svc = hyper_util::service::TowerToHyperService::new(grpc_service.clone());
                                        tokio::spawn(async move {
                                            let _ = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new())
                                                .serve_connection_with_upgrades(io, svc)
                                                .await;
                                        });
                                    }
                                }
                                _ = token.cancelled() => {
                                    break;
                                }
                            }
                        } else {
                            if let Ok((stream, _)) = grpc_listener.accept().await {
                                let io = hyper_util::rt::TokioIo::new(stream);
                                let svc = hyper_util::service::TowerToHyperService::new(
                                    grpc_service.clone(),
                                );
                                tokio::spawn(async move {
                                    let _ = hyper_util::server::conn::auto::Builder::new(
                                        hyper_util::rt::TokioExecutor::new(),
                                    )
                                    .serve_connection_with_upgrades(io, svc)
                                    .await;
                                });
                            }
                        }
                    }
                }
            });
        }

        self.plugin_registry.on_startup_all().await?;

        let chain = Arc::new(self.chain);
        let static_dir = self.static_dir;
        let metrics = self.metrics;
        let plugin_registry = Arc::new(self.plugin_registry);
        let wasm_middleware = self.wasm_middleware.clone();
        #[cfg(feature = "ws")]
        let ws_handler = self.ws_handler.clone();

        #[cfg(feature = "tls")]
        if let Some(tls_config) = self.tls {
            #[cfg(feature = "ws")]
            let res = serve_with_tls(
                listener,
                chain,
                tls_config,
                static_dir,
                metrics,
                self.shutdown,
                wasm_middleware,
                ws_handler,
            )
            .await;
            #[cfg(not(feature = "ws"))]
            let res = serve_with_tls(
                listener,
                chain,
                tls_config,
                static_dir,
                metrics,
                self.shutdown,
                wasm_middleware,
            )
            .await;
            plugin_registry.on_shutdown_all().await?;
            return res;
        }

        tracing::info!("Listening on {} (plain HTTP/1.1)", local_addr);
        #[cfg(feature = "ws")]
        let res = serve_http(
            listener,
            chain,
            static_dir,
            metrics,
            self.shutdown,
            wasm_middleware,
            ws_handler,
        )
        .await;
        #[cfg(not(feature = "ws"))]
        let res =
            serve_http(listener, chain, static_dir, metrics, self.shutdown, wasm_middleware).await;
        plugin_registry.on_shutdown_all().await?;
        res
    }
}

// ---------------------------------------------------------------------------
// Standalone helper
// ---------------------------------------------------------------------------

pub async fn serve(listener: TcpListener) -> Result<()> {
    let mut router: Router<Handler> = Router::new();
    router.set_fallback(Handler::Static {
        status: StatusCode::NOT_FOUND,
        body: r#"{"error":"not found"}"#,
    });
    router
        .insert(
            hyper::Method::GET,
            "/hello",
            Handler::Static { status: StatusCode::OK, body: r#"{"message":"hello"}"# },
        )
        .unwrap();
    router.insert(hyper::Method::POST, "/echo", Handler::Echo).unwrap();
    router.insert(hyper::Method::GET, "/health", Handler::Health).unwrap();
    router.insert(hyper::Method::GET, "/ready", Handler::Ready).unwrap();
    router.insert(hyper::Method::GET, "/live", Handler::Live).unwrap();
    router.insert(hyper::Method::GET, "/metrics", Handler::Prometheus).unwrap();
    router.insert(hyper::Method::GET, "/events", Handler::Sse).unwrap();

    let router = Arc::new(router);
    let pool = Arc::new(BufferPool::new());
    let metrics = Metrics::new();
    let health_registry = Arc::new(HealthRegistry::new());
    let handler = make_handler(router, pool, metrics.clone(), health_registry, None);
    let chain = Arc::new(MiddlewareChain::new(handler));

    #[cfg(feature = "ws")]
    let res = serve_http(listener, chain, None, metrics, None, None, Some(default_ws_echo())).await;
    #[cfg(not(feature = "ws"))]
    let res = serve_http(listener, chain, None, metrics, None, None).await;
    res
}

// ---------------------------------------------------------------------------
// Handler factory
// ---------------------------------------------------------------------------

fn make_handler(
    router: Arc<Router<Handler>>,
    pool: Arc<BufferPool>,
    metrics: Metrics,
    health_registry: Arc<HealthRegistry>,
    openapi_spec: Option<Arc<String>>,
) -> crate::middleware::HandlerFn {
    // Build the global GraphQL schema
    let graphql_schema = Arc::new(crate::graphql::create_schema());

    Arc::new(move |req: Request<Incoming>| {
        let router = router.clone();
        let pool = pool.clone();
        let metrics = metrics.clone();
        let health_registry = health_registry.clone();
        let graphql_schema = graphql_schema.clone();
        let openapi_spec = openapi_spec.clone();
        Box::pin(async move {
            let method = req.method().clone();
            let path = req.uri().path().to_string();
            match router.at(&method, &path) {
                Ok(m) => {
                    execute_handler(
                        m,
                        req,
                        &pool,
                        &metrics,
                        Some(&health_registry),
                        Some(&graphql_schema),
                        openapi_spec.as_deref().map(|s| s.as_str()),
                    )
                    .await
                }
                Err(crate::router::RouterError::MethodNotAllowed) => Ok(json_response(
                    StatusCode::METHOD_NOT_ALLOWED,
                    r#"{"error":"method not allowed"}"#,
                )),
                Err(crate::router::RouterError::NotFound) => {
                    let fb = router.fallback().unwrap();
                    let m = Match { handler: fb, params: matchit::Params::new() };
                    execute_handler(
                        m,
                        req,
                        &pool,
                        &metrics,
                        Some(&health_registry),
                        Some(&graphql_schema),
                        openapi_spec.as_deref().map(|s| s.as_str()),
                    )
                    .await
                }
            }
        })
    })
}

// ---------------------------------------------------------------------------
// HTTP server loop
// ---------------------------------------------------------------------------

/// Max time to wait for a client to send complete request headers (slowloris
/// protection). Applies to HTTP/1 and HTTP/2.
const HEADER_READ_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(30);

/// Max wall-clock time a single request handler may run before we respond 504
/// and abort it. Guards against stuck/slow handlers and resource exhaustion.
/// Configurable via `JUSTAPI_REQUEST_TIMEOUT_SECS`.
fn request_timeout() -> std::time::Duration {
    std::env::var("JUSTAPI_REQUEST_TIMEOUT_SECS")
        .ok()
        .and_then(|v| v.parse::<u64>().ok())
        .map(std::time::Duration::from_secs)
        .unwrap_or_else(|| std::time::Duration::from_secs(60))
}

/// Hard cap on concurrently accepted connections. Beyond this, new connections
/// block at accept time instead of letting the process exhaust memory/file
/// descriptors (connection-flood protection). Configurable via
/// `JUSTAPI_MAX_CONNECTIONS`.
fn max_connections() -> usize {
    std::env::var("JUSTAPI_MAX_CONNECTIONS")
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|n| *n > 0)
        .unwrap_or(10_000)
}

async fn serve_http(
    listener: TcpListener,
    chain: Arc<MiddlewareChain>,
    static_dir: Option<StaticDir>,
    metrics: Metrics,
    shutdown: Option<CancellationToken>,
    wasm_middleware: Option<Arc<crate::wasm::WasmEngine>>,
    #[cfg(feature = "ws")] ws_handler: Option<WsHandler>,
) -> Result<()> {
    let mut connections = tokio::task::JoinSet::new();
    let conn_semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_connections()));

    loop {
        let token = shutdown.as_ref().cloned();

        let (stream, peer) = if let Some(token) = &token {
            tokio::select! {
                result = listener.accept() => result?,
                _ = token.cancelled() => {
                    tracing::info!("Shutdown signal received, stopping accept loop");
                    break;
                }
            }
        } else {
            listener.accept().await?
        };
        tracing::debug!("Accepted connection from {}", peer);

        // --- Normal HTTP via hyper -----------------------------------------
        let chain = chain.clone();
        let static_dir = static_dir.clone();
        let conn_metrics = metrics.clone();
        conn_metrics.connection_opened();
        let spawn_metrics = conn_metrics.clone();
        let wasm_middleware = wasm_middleware.clone();
        let peer_addr = peer;

        #[cfg(feature = "ws")]
        let ws_handler = ws_handler.clone();

        let token_clone = shutdown.clone();
        let conn_semaphore = conn_semaphore.clone();
        connections.spawn(async move {
            // Bound concurrent connections: a permit is held for the life of
            // the connection so a flood of accepts can't exhaust FDs/memory
            // (connection-flood / slowloris resource exhaustion).
            let _permit = conn_semaphore
                .acquire_owned()
                .await
                .expect("connection semaphore closed");
            let io = TokioIo::new(stream);
            let arena = Arc::new(SharedArena::new());
            let svc = service_fn(move |mut req| {
                arena.reset();
                let chain = chain.clone();
                let arena = arena.clone();
                let static_dir = static_dir.clone();
                let metrics = spawn_metrics.clone();
                let wasm_middleware = wasm_middleware.clone();
                #[cfg(feature = "ws")]
                let ws_handler = ws_handler.clone();
                async move {
                    let path = req.uri().path().to_string();
                    let method = req.method().clone();
                    let _arena_path = arena.alloc_str(&path);
                    let start = std::time::Instant::now();
                    req.extensions_mut().insert(peer_addr);

                    let span = tracing::info_span!(
                        "http.request",
                        http.method = method.as_str(),
                        http.path = %path,
                        http.status_code = tracing::field::Empty,
                    );

                    metrics.record_request();

                    #[cfg(feature = "ws")]
                    if let Some(ref handler) = ws_handler {
                        let is_ws = req.headers().get(hyper::header::UPGRADE)
                            .and_then(|v| v.to_str().ok())
                            .map(|s| s.eq_ignore_ascii_case("websocket"))
                            .unwrap_or(false);

                        if is_ws {
                            if let Some(key) = req.headers().get("sec-websocket-key") {
                                let key_str = key.to_str().unwrap_or("");
                                let mut sha1 = sha1::Sha1::new();
                                use sha1::Digest;
                                sha1.update(key_str.as_bytes());
                                sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
                                use base64::Engine;
                                let accept_key = base64::engine::general_purpose::STANDARD.encode(sha1.finalize());

                                let mut res = crate::json_response(StatusCode::SWITCHING_PROTOCOLS, "");
                                res.headers_mut().insert(hyper::header::UPGRADE, hyper::header::HeaderValue::from_static("websocket"));
                                res.headers_mut().insert(hyper::header::CONNECTION, hyper::header::HeaderValue::from_static("upgrade"));
                                res.headers_mut().insert("sec-websocket-accept", hyper::header::HeaderValue::from_str(&accept_key).unwrap());

                                let handler = handler.clone();
                                let path_clone = path.clone();

                                tokio::task::spawn(async move {
                                    match hyper::upgrade::on(&mut req).await {
                                        Ok(upgraded) => {
                                            let upgraded_io = hyper_util::rt::TokioIo::new(upgraded);
                                            let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
                                                upgraded_io,
                                                tokio_tungstenite::tungstenite::protocol::Role::Server,
                                                None
                                            ).await;
                                            let (write, read) = ws_stream.split();
                                            dispatch_ws(Box::new(read), Box::new(write), path_clone, &handler).await;
                                        }
                                        Err(e) => tracing::error!("WebSocket upgrade error: {}", e),
                                    }
                                });
                                return Ok(res);
                            }
                        }
                    }

                    // WASM middleware (synchronous setup + one await)
                    if let Some(ref wasm) = wasm_middleware {
                        let wasm_span = tracing::debug_span!("wasm.middleware");
                        let mut hdrs = serde_json::Map::new();
                        for (k, v) in req.headers() {
                            if let Ok(v_str) = v.to_str() {
                                hdrs.insert(
                                    k.as_str().to_string(),
                                    serde_json::Value::String(v_str.to_string()),
                                );
                            }
                        }
                        let req_json = serde_json::json!({
                            "path": path,
                            "method": req.method().as_str(),
                            "headers": hdrs,
                        })
                        .to_string();

                        let wasm_result = {
                            let _ws = wasm_span.enter();
                            wasm.execute_middleware(&req_json).await
                        };

                        match wasm_result {
                            Ok(res_json) => {
                                if let Ok(parsed) =
                                    serde_json::from_str::<serde_json::Value>(&res_json)
                                {
                                    if let Some(status) = parsed.get("status") {
                                        if let Some(status_code) = status.as_u64() {
                                            if status_code != 200 {
                                                let body = parsed
                                                    .get("body")
                                                    .and_then(|v| v.as_str())
                                                    .unwrap_or("")
                                                    .to_string();
                                                span.record("http.status_code", status_code);
                                                metrics.record_status(
                                                    StatusCode::from_u16(status_code as u16)
                                                        .unwrap_or(StatusCode::FORBIDDEN),
                                                );
                                                metrics.record_latency(
                                                    start.elapsed().as_secs_f64() * 1000.0,
                                                );
                                                return Ok(json_response(
                                                    StatusCode::from_u16(status_code as u16)
                                                        .unwrap_or(StatusCode::FORBIDDEN),
                                                    &body,
                                                ));
                                            }
                                        }
                                    }
                                }
                            }
                            Err(e) => {
                                tracing::error!("WASM execution error: {:?}", e);
                            }
                        }
                    }

                    // Run middleware chain (async, no entered span across await)
                    let resp = tokio::time::timeout(request_timeout(), chain.run(req)).await;

                    match resp {
                        Ok(r) => match r {
                            Ok(response) => {
                                let status = response.status();
                                span.record("http.status_code", status.as_u16());
                                metrics.record_status(status);
                                metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);

                                // If the response is 404 and we have a static dir, try serving files
                                if status == StatusCode::NOT_FOUND {
                                    if let Some(ref sd) = static_dir {
                                        if let Some(file_path) = sd.resolve(&path) {
                                            if tokio::fs::metadata(&file_path)
                                                .await
                                                .map(|m| m.is_file())
                                                .unwrap_or(false)
                                            {
                                                return sd.serve_file(&file_path).await;
                                            }
                                        }
                                    }
                                }
                                Ok(response)
                            }
                            Err(_) => {
                                span.record("http.status_code", 404u16);
                                metrics.record_status(StatusCode::NOT_FOUND);
                                metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);

                                // Middleware error — try static files
                                if let Some(ref sd) = static_dir {
                                    if let Some(file_path) = sd.resolve(&path) {
                                        if tokio::fs::metadata(&file_path)
                                            .await
                                            .map(|m| m.is_file())
                                            .unwrap_or(false)
                                        {
                                            return sd.serve_file(&file_path).await;
                                        }
                                    }
                                }
                                Ok(json_response(
                                    StatusCode::NOT_FOUND,
                                    r#"{"error":"not found"}"#
                                ))
                            }
                        },
                        Err(_) => {
                            // Handler exceeded the request timeout.
                            span.record("http.status_code", 504u16);
                            metrics.record_status(StatusCode::GATEWAY_TIMEOUT);
                            metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);
                            tracing::warn!(
                                "Request to {} {} timed out after {:?}",
                                method,
                                path,
                                request_timeout()
                            );
                            Ok(json_response(
                                StatusCode::GATEWAY_TIMEOUT,
                                r#"{"error":"request timeout"}"#
                            ))
                        }
                    }
                }
            });
            let mut builder = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
            builder.http1().timer(hyper_util::rt::TokioTimer::new()).header_read_timeout(HEADER_READ_TIMEOUT);
            let mut conn = std::pin::pin!(builder.serve_connection_with_upgrades(io, svc));

            if let Some(token) = token_clone {
                tokio::select! {
                    res = &mut conn => {
                        if let Err(e) = res {
                            tracing::warn!("Connection error: {}", e);
                        }
                    }
                    _ = token.cancelled() => {
                        conn.as_mut().graceful_shutdown();
                        if let Err(e) = conn.await {
                            tracing::warn!("Connection error during shutdown: {}", e);
                        }
                    }
                }
            } else {
                if let Err(e) = conn.await {
                    tracing::warn!("Connection error: {}", e);
                }
            }
            conn_metrics.connection_closed();
        });
    }

    if shutdown.is_some() {
        tracing::info!("Waiting for {} active connections to drain...", connections.len());
        tokio::select! {
            _ = async { while connections.join_next().await.is_some() {} } => {
                tracing::debug!("all connections closed gracefully");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                tracing::warn!("Graceful shutdown timeout exceeded. Dropping remaining connections.");
            }
        }
    } else {
        while connections.join_next().await.is_some() {}
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Route handler
// ---------------------------------------------------------------------------

async fn execute_handler(
    m: Match<'_, '_, Handler>,
    req: Request<Incoming>,
    pool: &BufferPool,
    metrics: &Metrics,
    health_registry: Option<&HealthRegistry>,
    graphql_schema: Option<&crate::graphql::AppSchema>,
    openapi_spec: Option<&str>,
) -> Result<Response<ResponseBody>> {
    let handler_name = match m.handler {
        Handler::Static { .. } => "static",
        Handler::Echo => "echo",
        Handler::ParamsEcho => "params_echo",
        Handler::Sse => "sse",
        Handler::Health => "health",
        Handler::Ready => "ready",
        Handler::Live => "live",
        Handler::Prometheus => "prometheus",
        Handler::OpenApiJson => "openapi_json",
        Handler::SwaggerUi => "swagger_ui",
        Handler::Redoc => "redoc",
        Handler::GraphQL => "graphql",
        Handler::Custom(_) => "custom",
    };
    let _handler_span = tracing::debug_span!("handler.execute", name = handler_name);

    match m.handler {
        Handler::Static { status, body } => Ok(json_response(*status, body)),
        Handler::Echo => {
            let body_bytes = match http_body_util::Limited::new(req.into_body(), 50 * 1024 * 1024)
                .collect()
                .await
            {
                Ok(collected) => {
                    let bytes = collected.to_bytes();
                    metrics.add_bytes_in(bytes.len() as u64);
                    bytes
                }
                Err(_) => {
                    return Ok(json_response(StatusCode::BAD_REQUEST, r#"{"error":"bad request"}"#))
                }
            };
            let mut buf = pool.acquire(body_bytes.len());
            buf.extend_from_slice(&body_bytes);
            let body_str = String::from_utf8_lossy(&buf).to_string();
            pool.release(buf);
            Ok(json_response(StatusCode::OK, &body_str))
        }
        Handler::ParamsEcho => {
            let params_str: Vec<String> =
                m.params.iter().map(|(k, v)| format!(r#""{}":"{}""#, k, v)).collect();
            let body = format!("{{{}}}", params_str.join(","));
            Ok(json_response(StatusCode::OK, &body))
        }
        Handler::Sse => Ok(sse_response()),
        Handler::Health => Ok(metrics::health_response()),
        Handler::Ready => {
            if let Some(reg) = health_registry {
                if reg.is_empty() {
                    Ok(metrics::ready_response())
                } else {
                    Ok(reg.health_response().await)
                }
            } else {
                Ok(metrics::ready_response())
            }
        }
        Handler::Live => Ok(metrics::live_response()),
        Handler::Prometheus => Ok(metrics::metrics_response(metrics)),
        Handler::OpenApiJson => {
            let body: String =
                if let Some(spec) = openapi_spec { spec.to_string() } else { BUILTIN_SPEC.clone() };
            Ok(json_response(StatusCode::OK, &body))
        }
        Handler::SwaggerUi => {
            let html = openapi::swagger_ui_html();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html; charset=utf-8")
                .header("content-length", html.len().to_string())
                .body(crate::UnsyncBoxBody::new(
                    http_body_util::Full::new(Bytes::from(html))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap())
        }
        Handler::Redoc => {
            let html = openapi::redoc_html();
            Ok(Response::builder()
                .status(StatusCode::OK)
                .header("content-type", "text/html; charset=utf-8")
                .header("content-length", html.len().to_string())
                .body(crate::UnsyncBoxBody::new(
                    http_body_util::Full::new(Bytes::from(html))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap())
        }
        Handler::GraphQL => {
            if let Some(schema) = graphql_schema {
                crate::graphql::handle_graphql(schema, req).await.or_else(|e| {
                    Ok(Response::builder()
                        .status(StatusCode::INTERNAL_SERVER_ERROR)
                        .body(crate::UnsyncBoxBody::new(
                            http_body_util::Full::new(Bytes::from(format!("GraphQL Error: {}", e)))
                                .map_err(|e: std::convert::Infallible| -> anyhow::Error {
                                    match e {}
                                }),
                        ))
                        .unwrap())
                })
            } else {
                Ok(Response::builder()
                    .status(StatusCode::INTERNAL_SERVER_ERROR)
                    .body(crate::UnsyncBoxBody::new(
                        http_body_util::Full::new(Bytes::from("GraphQL schema not initialized"))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                    ))
                    .unwrap())
            }
        }
        Handler::Custom(f) => f(req).await,
    }
}

// ---------------------------------------------------------------------------
// SSE
// ---------------------------------------------------------------------------

fn sse_response() -> Response<ResponseBody> {
    let (tx, rx) = tokio::sync::mpsc::channel::<Result<Bytes>>(16);

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
async fn dispatch_ws(read: WsRead, write: WsWrite, path: String, handler: &WsHandler) {
    if let Err(e) = handler(path, read, write).await {
        tracing::warn!("WebSocket handler error: {}", e);
    }
}

// ---------------------------------------------------------------------------
// TLS
// ---------------------------------------------------------------------------

#[cfg(feature = "tls")]
fn init_tls_provider() {
    rustls::crypto::ring::default_provider()
        .install_default()
        .expect("ring crypto provider should install");
}

#[cfg(feature = "tls")]
async fn serve_with_tls(
    listener: TcpListener,
    chain: Arc<MiddlewareChain>,
    config: TlsConfig,
    static_dir: Option<StaticDir>,
    metrics: Metrics,
    shutdown: Option<CancellationToken>,
    wasm_middleware: Option<Arc<crate::wasm::WasmEngine>>,
    #[cfg(feature = "ws")] ws_handler: Option<WsHandler>,
) -> Result<()> {
    use std::fs::File;
    use std::io::BufReader;
    use std::sync::Arc as StdArc;

    init_tls_provider();

    let certs = {
        let mut reader = BufReader::new(File::open(&config.cert_path)?);
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?
    };
    let key = {
        let mut reader = BufReader::new(File::open(&config.key_path)?);
        rustls_pemfile::private_key(&mut reader)?
            .ok_or_else(|| anyhow::anyhow!("no private key in {}", config.key_path))?
    };

    let mut server_config =
        rustls::ServerConfig::builder().with_no_client_auth().with_single_cert(certs, key)?;

    server_config.alpn_protocols = vec![b"h2".to_vec(), b"http/1.1".to_vec()];

    let acceptor = tokio_rustls::TlsAcceptor::from(StdArc::new(server_config));

    let local_addr = listener.local_addr()?;
    tracing::info!("Listening on {} (TLS, HTTP/1.1 + HTTP/2 via ALPN)", local_addr);

    let mut connections = tokio::task::JoinSet::new();
    let conn_semaphore = std::sync::Arc::new(tokio::sync::Semaphore::new(max_connections()));

    loop {
        let token = shutdown.as_ref().cloned();

        let (stream, peer) = if let Some(token) = &token {
            tokio::select! {
                result = listener.accept() => result?,
                _ = token.cancelled() => {
                    tracing::info!("Shutdown signal received, stopping TLS accept loop");
                    break;
                }
            }
        } else {
            listener.accept().await?
        };
        let acceptor = acceptor.clone();
        let chain = chain.clone();
        let static_dir = static_dir.clone();
        let conn_metrics = metrics.clone();
        let wasm_middleware = wasm_middleware.clone();

        conn_metrics.connection_opened();
        let spawn_metrics = conn_metrics.clone();
        let peer_addr = peer;
        #[cfg(feature = "ws")]
        let ws_handler = ws_handler.clone();
        let token_clone = shutdown.clone();
        let conn_semaphore = conn_semaphore.clone();
        connections.spawn(async move {
            // Bound concurrent connections (see serve_http for rationale).
            let _permit = conn_semaphore
                .acquire_owned()
                .await
                .expect("connection semaphore closed");
            match acceptor.accept(stream).await {
                Ok(tls_stream) => {
                    let io = TokioIo::new(tls_stream);
                    let arena = Arc::new(SharedArena::new());
                    let svc = service_fn(move |mut req| {
                        arena.reset();
                        let chain = chain.clone();
                        let arena = arena.clone();
                        let static_dir = static_dir.clone();
                        let metrics = spawn_metrics.clone();
                        let wasm_middleware = wasm_middleware.clone();
                        #[cfg(feature = "ws")]
                        let ws_handler = ws_handler.clone();
                        async move {
                            let path = req.uri().path().to_string();
                            let _arena_path = arena.alloc_str(&path);
                            let start = std::time::Instant::now();
                            req.extensions_mut().insert(peer_addr);

                            metrics.record_request();

                            #[cfg(feature = "ws")]
                            if let Some(ref handler) = ws_handler {
                                let is_ws = req.headers().get(hyper::header::UPGRADE)
                                    .and_then(|v| v.to_str().ok())
                                    .map(|s| s.eq_ignore_ascii_case("websocket"))
                                    .unwrap_or(false);

                                if is_ws {
                                    if let Some(key) = req.headers().get("sec-websocket-key") {
                                        let key_str = key.to_str().unwrap_or("");
                                        let mut sha1 = sha1::Sha1::new();
                                        use sha1::Digest;
                                        sha1.update(key_str.as_bytes());
                                        sha1.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
                                        use base64::Engine;
                                        let accept_key = base64::engine::general_purpose::STANDARD.encode(sha1.finalize());

                                        let mut res = crate::json_response(StatusCode::SWITCHING_PROTOCOLS, "");
                                        res.headers_mut().insert(hyper::header::UPGRADE, hyper::header::HeaderValue::from_static("websocket"));
                                        res.headers_mut().insert(hyper::header::CONNECTION, hyper::header::HeaderValue::from_static("upgrade"));
                                        res.headers_mut().insert("sec-websocket-accept", hyper::header::HeaderValue::from_str(&accept_key).unwrap());

                                        let handler = handler.clone();
                                        let path_clone = path.clone();

                                        tokio::task::spawn(async move {
                                            match hyper::upgrade::on(&mut req).await {
                                                Ok(upgraded) => {
                                                    let upgraded_io = hyper_util::rt::TokioIo::new(upgraded);
                                                    let ws_stream = tokio_tungstenite::WebSocketStream::from_raw_socket(
                                                        upgraded_io,
                                                        tokio_tungstenite::tungstenite::protocol::Role::Server,
                                                        None
                                                    ).await;
                                                    let (write, read) = ws_stream.split();
                                                    dispatch_ws(Box::new(read), Box::new(write), path_clone, &handler).await;
                                                }
                                                Err(e) => tracing::error!("WebSocket upgrade error: {}", e),
                                            }
                                        });
                                        return Ok(res);
                                    }
                                }
                            }

                            if let Some(ref wasm) = wasm_middleware {
                                let mut hdrs = serde_json::Map::new();
                                for (k, v) in req.headers() {
                                    if let Ok(v_str) = v.to_str() {
                                        hdrs.insert(
                                            k.as_str().to_string(),
                                            serde_json::Value::String(v_str.to_string()),
                                        );
                                    }
                                }
                                let req_json = serde_json::json!({
                                    "path": path,
                                    "method": req.method().as_str(),
                                    "headers": hdrs,
                                })
                                .to_string();

                                match wasm.execute_middleware(&req_json).await {
                                    Ok(res_json) => {
                                        if let Ok(parsed) =
                                            serde_json::from_str::<serde_json::Value>(&res_json)
                                        {
                                            if let Some(status) = parsed.get("status") {
                                                if let Some(status_code) = status.as_u64() {
                                                    if status_code != 200 {
                                                        let body = parsed
                                                            .get("body")
                                                            .and_then(|v| v.as_str())
                                                            .unwrap_or("")
                                                            .to_string();
                                                        metrics.record_status(
                                                            StatusCode::from_u16(
                                                                status_code as u16,
                                                            )
                                                            .unwrap_or(StatusCode::FORBIDDEN),
                                                        );
                                                        metrics.record_latency(
                                                            start.elapsed().as_secs_f64() * 1000.0,
                                                        );
                                                        return Ok(json_response(
                                                            StatusCode::from_u16(
                                                                status_code as u16,
                                                            )
                                                            .unwrap_or(StatusCode::FORBIDDEN),
                                                            &body,
                                                        ));
                                                    }
                                                }
                                            }
                                        }
                                    }
                                    Err(e) => {
                                        tracing::error!("WASM execution error: {:?}", e);
                                    }
                                }
                            }

                            let resp = tokio::time::timeout(request_timeout(), chain.run(req)).await;

                            match resp {
                                Ok(r) => match r {
                                    Ok(response) => {
                                    let status = response.status();
                                    metrics.record_status(status);
                                    metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);

                                    if status == StatusCode::NOT_FOUND {
                                        if let Some(ref sd) = static_dir {
                                            if let Some(file_path) = sd.resolve(&path) {
                                                if tokio::fs::metadata(&file_path)
                                                    .await
                                                    .map(|m| m.is_file())
                                                    .unwrap_or(false)
                                                {
                                                    return sd.serve_file(&file_path).await;
                                                }
                                            }
                                        }
                                    }
                                    Ok(response)
                                }
                                Err(_) => {
                                    metrics.record_status(StatusCode::NOT_FOUND);
                                    metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);

                                    if let Some(ref sd) = static_dir {
                                        if let Some(file_path) = sd.resolve(&path) {
                                            if tokio::fs::metadata(&file_path)
                                                .await
                                                .map(|m| m.is_file())
                                                .unwrap_or(false)
                                            {
                                                return sd.serve_file(&file_path).await;
                                            }
                                        }
                                    }
                                    Ok(json_response(
                                        StatusCode::NOT_FOUND,
                                        r#"{"error":"not found"}"#,
                                    ))
                                },
                        },
                        Err(_) => {
                            // Handler exceeded the request timeout.
                            metrics.record_status(StatusCode::GATEWAY_TIMEOUT);
                            metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);
                            tracing::warn!(
                                "Request to {} {} timed out after {:?}",
                                method,
                                path,
                                request_timeout()
                            );
                            Ok(json_response(
                                StatusCode::GATEWAY_TIMEOUT,
                                r#"{"error":"request timeout"}"#
                            ))
                        }
                    }
                    }
                });

                    let mut builder = hyper_util::server::conn::auto::Builder::new(hyper_util::rt::TokioExecutor::new());
                    builder.http1().timer(hyper_util::rt::TokioTimer::new()).header_read_timeout(HEADER_READ_TIMEOUT);
                    let mut conn = std::pin::pin!(builder.serve_connection_with_upgrades(io, svc));

                    if let Some(token) = token_clone {
                        tokio::select! {
                            res = &mut conn => {
                                if let Err(e) = res {
                                    tracing::warn!("TLS connection error from {}: {}", peer, e);
                                }
                            }
                            _ = token.cancelled() => {
                                conn.as_mut().graceful_shutdown();
                                if let Err(e) = conn.await {
                                    tracing::warn!("TLS connection error during shutdown: {}", e);
                                }
                            }
                        }
                    } else {
                        if let Err(e) = conn.await {
                            tracing::warn!("TLS connection error from {}: {}", peer, e);
                        }
                    }
                }
                Err(e) => {
                    tracing::warn!("TLS handshake failed from {}: {}", peer, e);
                }
            }
            conn_metrics.connection_closed();
        });
    }

    if let Some(_) = shutdown {
        tracing::info!("Waiting for {} active TLS connections to drain...", connections.len());
        tokio::select! {
            _ = async { while connections.join_next().await.is_some() {} } => {
                tracing::debug!("all connections closed after fallback drain");
            }
            _ = tokio::time::sleep(std::time::Duration::from_secs(30)) => {
                tracing::warn!("Graceful shutdown timeout exceeded. Dropping remaining TLS connections.");
            }
        }
    } else {
        while connections.join_next().await.is_some() {}
    }

    Ok(())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::router::Router;

    #[tokio::test]
    async fn test_hello_route_via_router() {
        let mut router: Router<Handler> = Router::new();
        router
            .insert(
                Method::GET,
                "/hello",
                Handler::Static { status: StatusCode::OK, body: r#"{"message":"hello"}"# },
            )
            .unwrap();
        let m = router.at(&Method::GET, "/hello").unwrap();
        assert!(matches!(m.handler, Handler::Static { .. }));
    }

    #[tokio::test]
    async fn test_unknown_route_fallback() {
        let mut router: Router<Handler> = Router::new();
        router.set_fallback(Handler::Static {
            status: StatusCode::NOT_FOUND,
            body: r#"{"error":"not found"}"#,
        });
        let fb = router.fallback().unwrap();
        assert!(matches!(fb, Handler::Static { .. }));
    }

    #[tokio::test]
    async fn test_users_param_route() {
        let mut router: Router<Handler> = Router::new();
        router
            .insert(
                Method::GET,
                "/users/{id}",
                Handler::Static { status: StatusCode::OK, body: r#"{"message":"user lookup"}"# },
            )
            .unwrap();
        let m = router.at(&Method::GET, "/users/42").unwrap();
        assert_eq!(m.params.get("id"), Some("42"));
    }

    #[tokio::test]
    async fn test_arena_reuse() {
        let arena = SharedArena::new();
        arena.alloc_str("/hello").unwrap();
        arena.reset();
        let s = arena.alloc_str("/users/42").unwrap();
        assert_eq!(s, "/users/42");
    }

    #[tokio::test]
    async fn test_sse_route_exists() {
        let mut router: Router<Handler> = Router::new();
        router.insert(Method::GET, "/events", Handler::Sse).unwrap();
        let m = router.at(&Method::GET, "/events").unwrap();
        assert!(matches!(m.handler, Handler::Sse));
    }

    #[tokio::test]
    async fn test_health_route_exists() {
        let mut router: Router<Handler> = Router::new();
        router.insert(Method::GET, "/health", Handler::Health).unwrap();
        let m = router.at(&Method::GET, "/health").unwrap();
        assert!(matches!(m.handler, Handler::Health));
    }

    #[tokio::test]
    async fn test_prometheus_route_exists() {
        let mut router: Router<Handler> = Router::new();
        router.insert(Method::GET, "/metrics", Handler::Prometheus).unwrap();
        let m = router.at(&Method::GET, "/metrics").unwrap();
        assert!(matches!(m.handler, Handler::Prometheus));
    }
}

// --- gRPC stub placeholder ---
// (We'll fully integrate the gRPC side-by-side loop next)
