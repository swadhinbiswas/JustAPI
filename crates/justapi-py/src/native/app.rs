use hyper::Method;
use std::collections::HashMap;
use std::sync::{Arc, Mutex, OnceLock};

use jsonschema::Validator;
use pyo3::conversion::IntoPyObject;
use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList, PyString, PyTuple};
use tokio_util::sync::CancellationToken;
use uuid::Uuid;

/// Cached tokio runtime for synchronous pyclass methods that need async execution
/// (e.g. graphql_handle, health_ready). Created once to avoid the ~1-2ms runtime
/// creation cost on every call.
static CACHED_TOKIO_RT: std::sync::OnceLock<tokio::runtime::Runtime> = std::sync::OnceLock::new();

/// Await a SIGTERM (Unix only). Returns when the process receives the signal.
#[cfg(unix)]
async fn term_signal() {
    use tokio::signal::unix::{signal, SignalKind};
    match signal(SignalKind::terminate()) {
        Ok(mut sig) => {
            sig.recv().await;
        }
        Err(e) => {
            // If we can't install the handler, just block forever so shutdown
            // still happens via Ctrl+C. Don't crash the server over it.
            tracing::warn!("Failed to install SIGTERM handler: {}", e);
            std::future::pending::<()>().await;
        }
    }
}

use crate::request::Conn;
use crate::websocket::{WebSocket, WsMessage};
use futures::{SinkExt, StreamExt};
use tokio_tungstenite::tungstenite::protocol::CloseFrame;

use justapi_core::router::Router;

use crate::database::Database;

use super::handlers::*;
use super::types::*;

#[pyclass(name = "_JustAPIApp")]
pub struct JustAPIApp {
    pub router: Router<usize>,
    pub handlers: Vec<Py<PyAny>>,
    pub native: Vec<bool>,
    /// Per-handler Rust-native CRUD config (ADR-056 Step C). `Some((table,
    /// columns, id_column))` means this route is served entirely in Rust by
    /// `crud_dispatch_bytes`, with no GIL hop. The operation is inferred from
    /// the request method at runtime. Indexed by handler id.
    pub crud: Vec<Option<(String, Vec<String>, String)>>,
    /// Rust-native SSE stream spec per route: `Some((count, interval_ms))`
    /// means the route serves a Rust-generated SSE stream with no Python
    /// involvement (ADR-088). Indexed by handler id.
    pub sse_specs: Vec<Option<(u64, u64)>>,
    pub schemas: Vec<Option<Py<PyAny>>>,
    pub schema_jsons: Vec<Option<String>>,
    /// JSON Schemas (resolved at registration) used by the native fast path to
    /// validate a request's *query string* in Rust, with no GIL/Python hop.
    pub query_schema_jsons: Vec<Option<String>>,
    pub batch_configs: Vec<Option<(usize, u64)>>,
    pub plugins: Vec<Py<PyAny>>,
    pub database: Option<Database>,
    /// Resolved DB pool (set once `connect_database` or `run()` resolves the
    /// configured database), exposed to Python handlers via the `DbPool` bridge so
    /// application code can run arbitrary, injection-safe SQL without losing the
    /// GIL-avoidance win (ADR-056 follow-up). Available before `run()` if the pool
    /// was connected eagerly.
    pub db_pool: Option<crate::database::DbPool>,
    /// Persistent tokio runtime that owns the `db_pool` handle when the pool was
    /// connected eagerly (outside `run()`). Kept alive for the app's lifetime so
    /// the `DbPool`'s `Handle` stays valid for the whole process.
    pub db_runtime: Option<tokio::runtime::Runtime>,
    pub grpc_addr: Option<std::net::SocketAddr>,
    pub grpc_handlers: std::collections::HashMap<String, Py<PyAny>>,
    pub ws_routes: std::collections::HashMap<String, Py<PyAny>>,
    pub wasm_bytes: Option<Vec<u8>>,
    pub gateway_config: Option<String>,
    pub circuit_breaker_config: Option<(usize, u64)>,
    pub coalesce_headers: Option<Vec<String>>,
    /// HTTP/3 (QUIC) TLS certificate/key paths, set via `enable_http3(...)`.
    /// When Some, `run()` also starts a UDP/QUIC listener serving the same
    /// application handler over HTTP/3 (feature `http3` on justapi-core).
    pub http3_cert: Option<String>,
    pub http3_key: Option<String>,
    /// When true, apply safe (non-HSTS) security headers to every response by
    /// default. Off by default because forcing a CSP would break apps that load
    /// external resources (e.g. CDN-hosted docs UIs). Call
    /// `enable_secure_headers()` to opt in.
    pub secure_headers: bool,
    /// Explicit security-headers config (e.g. with HSTS). When `secure_headers`
    /// is true but this is `None`, the safe non-HSTS default is used.
    pub secure_headers_config: Option<justapi_core::middleware::SecurityHeaders>,
    /// Live metrics collector, populated just before the server starts so the
    /// Python `/metrics` builtin can export real Prometheus data.
    pub metrics: Option<justapi_core::metrics::Metrics>,
    /// Health registry, populated just before the server starts so the Python
    /// `/ready` builtin reflects registered dependency checks.
    pub health_registry: Option<std::sync::Arc<justapi_core::health::HealthRegistry>>,
    /// Health checks registered from Python via `register_health_check`, applied
    /// to the server's registry at startup.
    pub health_checks: Vec<(String, Py<PyAny>)>,
    /// Per-route OpenAPI metadata, keyed by `(method, path)`.
    pub route_meta:
        std::collections::HashMap<(hyper::Method, String), justapi_core::openapi::RouteMeta>,
    /// Named routes for `url_for` resolution, keyed by name -> path template.
    pub named_routes: std::collections::HashMap<String, String>,
    /// Map of `(method, path)` -> handler index, allowing user routes to override built-in routes cleanly.
    pub route_indices: std::collections::HashMap<(hyper::Method, String), usize>,
    /// Static frontend mounts (served as low-priority routes with SPA fallback).
    pub frontend_mounts: Vec<justapi_core::static_files::StaticMount>,
    /// Native MCP tool registry (agent surface). Stored in Rust; invoked as
    /// Python callables via PyO3. See `register_tool` / `list_tools` /
    /// `call_tool`.
    pub tools: Vec<PyTool>,
    /// Agent session state store. Keyed by session id; values are arbitrary
    /// JSON. Exposed to Python via `app.create_session` / `get_session` / etc.
    /// and to handlers via the `Session` dependency.
    pub sessions: Mutex<HashMap<String, serde_json::Value>>,
    /// Optional JWT authentication configuration. When set, the Rust
    /// ``JwtAuth`` middleware validates every request (skipping routes
    /// explicitly opted out). Decoded claims are bridged into
    /// ``request["auth"]``.
    pub jwt_auth: Option<justapi_core::middleware::JwtAuth>,
    /// Optional CORS configuration. When set, the Rust ``Cors`` middleware
    /// handles OPTIONS preflight requests and injects CORS response headers.
    /// Configured from Python via ``app.add_cors(...)``.
    pub cors: Option<justapi_core::middleware::Cors>,
}

/// A registered MCP tool: metadata plus its Python handler.
pub struct PyTool {
    pub name: String,
    pub description: String,
    pub schema: String,
    pub handler: Py<PyAny>,
}

/// Build a Rust-native CRUD config from the Python-side `crud_table` /
/// `crud_columns` arguments (both required together). The actual operation
/// (insert/select/update/delete) is inferred from the request method at
/// runtime in the handler dispatch. Returns `None` when neither is given (a
/// normal Python/native-echo route). Errors if only one is provided, or if
/// `crud_columns` is empty.
pub(crate) fn make_crud_spec(
    crud_table: Option<String>,
    crud_columns: Option<Vec<String>>,
) -> PyResult<Option<(String, Vec<String>, String)>> {
    match (crud_table, crud_columns) {
        (Some(table), Some(columns)) => {
            if columns.is_empty() {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "crud_columns must be non-empty when crud_table is set",
                ));
            }
            // `id` is the conventional primary-key column used for update/delete
            // by path id; configurable later if needed.
            Ok(Some((table, columns, "id".to_string())))
        }
        (Some(_), None) => {
            Err(pyo3::exceptions::PyValueError::new_err("crud_table requires crud_columns"))
        }
        (None, Some(_)) => {
            Err(pyo3::exceptions::PyValueError::new_err("crud_columns requires crud_table"))
        }
        (None, None) => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Python-registered health checks
// ---------------------------------------------------------------------------

/// A `HealthCheck` backed by a Python callable. The callable is invoked
/// synchronously under the GIL; a truthy return means healthy, a falsy return
/// or an exception means unhealthy (with the detail captured in the report).
struct PyHealthCheck {
    name: &'static str,
    check: Py<PyAny>,
}

impl justapi_core::health::HealthCheck for PyHealthCheck {
    fn name(&self) -> &'static str {
        self.name
    }

    fn check(
        &self,
    ) -> impl std::future::Future<Output = justapi_core::health::HealthStatus> + Send {
        let name = self.name;
        let check = Python::attach(|py| self.check.clone_ref(py));
        async move {
            Python::attach(|py| {
                let check = check.clone_ref(py);
                match check.bind(py).call0() {
                    Ok(v) => match v.is_truthy() {
                        Ok(true) => justapi_core::health::HealthStatus::Healthy,
                        Ok(false) => justapi_core::health::HealthStatus::Unhealthy(format!(
                            "health check '{}' reported unhealthy",
                            name
                        )),
                        Err(e) => justapi_core::health::HealthStatus::Unhealthy(format!(
                            "health check '{}' eval error: {}",
                            name, e
                        )),
                    },
                    Err(e) => justapi_core::health::HealthStatus::Unhealthy(format!(
                        "health check '{}' raised: {}",
                        name, e
                    )),
                }
            })
        }
    }
}

// ---------------------------------------------------------------------------
// Streaming structured-output validation
// ---------------------------------------------------------------------------
//
// Each streamed item (a JSON value) is validated against a JSON Schema before
// it is forwarded to the client. Compiled `Validator`s are cached per schema
// string so a long stream does not recompile the schema on every item.

fn schema_validator_cache() -> &'static Mutex<HashMap<String, Validator>> {
    static CACHE: OnceLock<Mutex<HashMap<String, Validator>>> = OnceLock::new();
    CACHE.get_or_init(|| Mutex::new(HashMap::new()))
}

/// A `format` keyword checker: maps a string to a pass/fail boolean.
type FormatChecker = dyn Fn(&str) -> bool + Send + Sync;

/// Register the built-in `format` validators (email, uuid, uri, date-time,
/// ...) once. `jsonschema` 0.46 ships `format` as an opt-in keyword;
/// with `default-features = false` no formats are asserted unless we
/// register them, so we provide lightweight Rust-side checkers. They are
/// intentionally permissive (reject obvious junk, not RFC-exhaustive) —
/// enough for request validation without pulling a format-parsing dependency.
fn format_validators() -> &'static HashMap<String, Arc<FormatChecker>> {
    static FMTS: OnceLock<HashMap<String, Arc<FormatChecker>>> = OnceLock::new();
    FMTS.get_or_init(|| {
        let mut m: HashMap<String, Arc<FormatChecker>> = HashMap::new();
        m.insert(
            "email".into(),
            Arc::new(|s: &str| {
                if s.is_empty() || s.contains(char::is_whitespace) {
                    return false;
                }
                let (local, domain) = match s.split_once('@') {
                    Some(pair) => pair,
                    None => return false,
                };
                !local.is_empty()
                    && !domain.is_empty()
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            }),
        );
        m.insert(
            "uri".into(),
            Arc::new(|s: &str| match s.split_once("://") {
                Some((scheme, rest)) => {
                    !scheme.is_empty()
                        && scheme
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '+' || c == '-' || c == '.')
                        && !rest.is_empty()
                }
                None => false,
            }),
        );
        m.insert(
            "uuid".into(),
            Arc::new(|s: &str| {
                let s = s.trim_matches(|c| c == '{' || c == '}');
                let parts: Vec<&str> = s.split('-').collect();
                parts.len() == 5
                    && parts[0].len() == 8
                    && parts[1].len() == 4
                    && parts[2].len() == 4
                    && parts[3].len() == 4
                    && parts[4].len() == 12
                    && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
            }),
        );
        m.insert(
            "date-time".into(),
            Arc::new(|s: &str| {
                let (date, time) = match s.split_once('T') {
                    Some((d, t)) => (d, t),
                    None => return false,
                };
                let ds: Vec<&str> = date.split('-').collect();
                if ds.len() != 3
                    || ds.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
                {
                    return false;
                }
                let time = time.trim_end_matches('Z').trim_end_matches(['+', '-']);
                let time = time.split('.').next().unwrap_or(time);
                let ts: Vec<&str> = time.split(':').collect();
                ts.len() >= 2
                    && ts.len() <= 3
                    && ts.iter().all(|p| {
                        p.len() <= 2 && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())
                    })
            }),
        );
        m.insert(
            "date".into(),
            Arc::new(|s: &str| {
                let ds: Vec<&str> = s.split('-').collect();
                ds.len() == 3
                    && ds.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            }),
        );
        m.insert(
            "hostname".into(),
            Arc::new(|s: &str| {
                !s.is_empty()
                    && !s.starts_with('-')
                    && !s.ends_with('-')
                    && !s.contains("..")
                    && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.')
            }),
        );
        m.insert(
            "ipv4".into(),
            Arc::new(|s: &str| {
                let parts: Vec<&str> = s.split('.').collect();
                parts.len() == 4
                    && parts.iter().all(|p| {
                        p.len() <= 3
                            && !p.is_empty()
                            && p.chars().all(|c| c.is_ascii_digit())
                            && p.parse::<u8>().is_ok()
                    })
            }),
        );
        m
    })
}

/// Build (and cache) a validator for `schema_value`, registering the built-in
/// `format` checkers so `format: "email"` etc. are actually asserted.
fn build_validator(schema_json: &str, schema_value: &serde_json::Value) -> PyResult<Validator> {
    {
        let cache = schema_validator_cache().lock().unwrap_or_else(|e| e.into_inner());
        if let Some(v) = cache.get(schema_json) {
            return Ok(v.clone());
        }
    }
    let mut opts = jsonschema::options();
    for (name, checker) in format_validators() {
        let checker = checker.clone();
        opts = opts.with_format(name.clone(), move |s: &str| checker(s));
    }
    let v = opts
        .should_validate_formats(true)
        .build(schema_value)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("schema error: {e}")))?;
    schema_validator_cache()
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .insert(schema_json.to_string(), v.clone());
    Ok(v)
}

/// Validate a single JSON value (`value_json`) against a JSON Schema
/// (`schema_json`). Returns a list of human-readable error strings; an empty
/// list means the value is valid. The compiled schema is cached per unique
/// schema string. `format` keywords (email, uuid, uri, ...) are enforced
/// via the built-in validators in `format_validators`.
#[pyfunction]
pub fn validate_value(schema_json: String, value_json: String) -> PyResult<Vec<String>> {
    let schema_value: serde_json::Value = serde_json::from_str(&schema_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("invalid schema: {e}")))?;
    let value: serde_json::Value = serde_json::from_str(&value_json)
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("invalid JSON value: {e}")))?;

    let validator = build_validator(&schema_json, &schema_value)?;

    let mut errors = Vec::new();
    for err in validator.iter_errors(&value) {
        errors.push(err.to_string());
    }
    Ok(errors)
}

/// A streaming response that validates each yielded JSON object against a schema
/// before forwarding it. Yielded items must be JSON-serialisable Python objects.
/// Emitted bytes are NDJSON (one object per line) or a JSON array, per `mode`.
#[pyclass(name = "ValidatedStreamResponse", subclass)]
pub struct ValidatedStreamResponse {
    pub generator: Py<PyAny>,
    pub schema_json: String,
    pub mode: String,
    pub status: u16,
    pub headers: Vec<(Vec<u8>, Vec<u8>)>,
}

#[pymethods]
impl ValidatedStreamResponse {
    #[new]
    #[pyo3(signature = (generator, schema_json, mode="ndjson", status=200, headers=None))]
    pub fn new(
        generator: Py<PyAny>,
        schema_json: String,
        mode: &str,
        status: u16,
        headers: Option<Vec<(Vec<u8>, Vec<u8>)>>,
    ) -> PyResult<Self> {
        // Validate the schema compiles up front.
        let _: serde_json::Value = serde_json::from_str(&schema_json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid tool schema: {e}"))
        })?;
        let content_type = match mode {
            "array" => b"application/json".to_vec(),
            _ => b"application/x-ndjson".to_vec(),
        };
        let headers = headers.unwrap_or_else(|| vec![(b"content-type".to_vec(), content_type)]);
        Ok(Self { generator, schema_json, mode: mode.to_string(), status, headers })
    }
}

/// Convert a parsed JSON value into a Python object (used to build tool-call
/// kwargs from a JSON-encoded arguments string).
fn json_value_to_py<'py>(py: Python<'py>, v: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    match v {
        serde_json::Value::Null => Ok(py.None().into_bound(py)),
        serde_json::Value::Bool(b) => Ok((*b).into_pyobject(py)?.as_any().clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.as_any().clone())
            } else if let Some(u) = n.as_u64() {
                Ok(u.into_pyobject(py)?.as_any().clone())
            } else {
                Ok(n.as_f64().unwrap_or(0.0).into_pyobject(py)?.as_any().clone())
            }
        }
        serde_json::Value::String(s) => Ok(s.clone().into_pyobject(py)?.as_any().clone()),
        serde_json::Value::Array(a) => {
            let list = PyList::empty(py);
            for item in a {
                list.append(json_value_to_py(py, item)?)?;
            }
            Ok(list.as_any().clone())
        }
        serde_json::Value::Object(o) => {
            let d = PyDict::new(py);
            for (k, val) in o {
                d.set_item(k, json_value_to_py(py, val)?)?;
            }
            Ok(d.as_any().clone())
        }
    }
}

#[pymethods]
impl JustAPIApp {
    #[new]
    fn new() -> Self {
        Self {
            router: Router::new(),
            handlers: Vec::new(),
            native: Vec::new(),
            crud: Vec::new(),
            sse_specs: Vec::new(),
            schemas: Vec::new(),
            schema_jsons: Vec::new(),
            query_schema_jsons: Vec::new(),
            batch_configs: Vec::new(),
            plugins: Vec::new(),
            database: None,
            db_pool: None,
            db_runtime: None,
            grpc_addr: None,
            grpc_handlers: std::collections::HashMap::new(),
            ws_routes: std::collections::HashMap::new(),
            wasm_bytes: None,
            gateway_config: None,
            circuit_breaker_config: None,
            coalesce_headers: None,
            http3_cert: None,
            http3_key: None,
            secure_headers: false,
            secure_headers_config: None,
            metrics: None,
            health_registry: None,
            health_checks: Vec::new(),
            route_meta: std::collections::HashMap::new(),
            named_routes: std::collections::HashMap::new(),
            route_indices: std::collections::HashMap::new(),
            frontend_mounts: Vec::new(),

            tools: Vec::new(),
            sessions: Mutex::new(HashMap::new()),
            jwt_auth: None,
            cors: None,
        }
    }

    fn _set_jwt_auth(
        &mut self,
        py: Python<'_>,
        secret: String,
        algorithm: Option<String>,
    ) -> PyResult<()> {
        let alg = match algorithm.as_deref() {
            Some("HS384") => jsonwebtoken::Algorithm::HS384,
            Some("HS512") => jsonwebtoken::Algorithm::HS512,
            Some("RS256") => jsonwebtoken::Algorithm::RS256,
            Some("RS384") => jsonwebtoken::Algorithm::RS384,
            Some("RS512") => jsonwebtoken::Algorithm::RS512,
            Some("ES256") => jsonwebtoken::Algorithm::ES256,
            Some("ES384") => jsonwebtoken::Algorithm::ES384,
            Some("EdDSA") | Some("ED25519") => jsonwebtoken::Algorithm::EdDSA,
            _ => jsonwebtoken::Algorithm::HS256,
        };
        let mut v = jsonwebtoken::Validation::new(alg);
        v.validate_exp = true;
        self.jwt_auth = Some(
            justapi_core::middleware::JwtAuth::from_secret(secret.as_bytes())
                .with_algorithms(vec![alg]),
        );
        let _ = py;
        Ok(())
    }

    /// Configure CORS from Python. ``allow_origins`` etc. are the same
    /// params the Python ``JustAPIApp.add_cors()`` receives; they are
    /// forwarded here to build a Rust ``Cors`` middleware that handles
    /// both OPTIONS preflight and per-response header injection.
    #[allow(clippy::too_many_arguments)]
    fn _set_cors(
        &mut self,
        py: Python<'_>,
        allow_origins: Option<Vec<String>>,
        allow_methods: Option<Vec<String>>,
        allow_headers: Option<Vec<String>>,
        allow_credentials: Option<bool>,
        expose_headers: Option<Vec<String>>,
        max_age: Option<u64>,
    ) -> PyResult<()> {
        let mut cors =
            if allow_origins.as_deref() == Some(&["*".to_string()]) || allow_origins.is_none() {
                justapi_core::middleware::Cors::permissive()
            } else {
                let mut c = justapi_core::middleware::Cors::new();
                if let Some(origins) = &allow_origins {
                    for o in origins {
                        c = c.allow_origin(o);
                    }
                }
                c
            };
        if let Some(methods) = allow_methods {
            cors = cors.allow_methods(&methods.join(", "));
        }
        if let Some(headers) = allow_headers {
            cors = cors.allow_headers(&headers.join(", "));
        }
        if let Some(creds) = allow_credentials {
            if creds {
                cors = cors.allow_credentials();
            }
        }
        if let Some(expose) = expose_headers {
            let refs: Vec<&str> = expose.iter().map(|s| s.as_str()).collect();
            cors = cors.expose_headers(&refs);
        }
        if let Some(age) = max_age {
            cors = cors.max_age(&age.to_string());
        }
        self.cors = Some(cors);
        let _ = py;
        Ok(())
    }

    /// Store OpenAPI metadata for a route (keyed by `method` + `path`).
    #[allow(clippy::too_many_arguments)]
    fn store_meta(
        &mut self,
        py: Python<'_>,
        method: String,
        path: String,
        body_schema: Option<Py<PyAny>>,
        response_schema_json: Option<Option<String>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        experimental: bool,
        include_in_schema: Option<bool>,
    ) {
        use justapi_core::openapi::RouteMeta;

        let method_enum =
            hyper::Method::from_bytes(method.as_bytes()).unwrap_or(hyper::Method::GET);
        let request_body_schema = resolve_schema_json(py, body_schema)
            .ok()
            .flatten()
            .as_deref()
            .and_then(|s| serde_json::from_str(s).ok());
        let response_schema =
            response_schema_json.flatten().as_deref().and_then(|s| serde_json::from_str(s).ok());
        let responses_val = responses.as_deref().and_then(|s| serde_json::from_str(s).ok());
        let openapi_extra_val = openapi_extra.as_deref().and_then(|s| serde_json::from_str(s).ok());

        self.route_meta.insert(
            (method_enum.clone(), path.clone()),
            RouteMeta {
                method: method_enum,
                path,
                summary,
                description,
                tags: tags.unwrap_or_default(),
                request_body_schema,
                response_schema,
                deprecated: deprecated.unwrap_or(false),
                experimental,
                status_code,
                responses: responses_val,
                operation_id,
                openapi_extra: openapi_extra_val,
                include_in_schema: include_in_schema.unwrap_or(true),
            },
        );
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

    /// Enable an HTTP/3 (QUIC) listener on the same address `run()` binds
    /// (UDP, same port). `cert_path`/`key_path` are PEM TLS files (required —
    /// QUIC uses TLS 1.3). The same application handler serves both transports.
    #[cfg(feature = "http3")]
    #[pyo3(name = "enable_http3")]
    fn enable_http3(&mut self, cert_path: String, key_path: String) {
        self.http3_cert = Some(cert_path);
        self.http3_key = Some(key_path);
    }

    /// Apply safe security headers (`X-Content-Type-Options`, `X-Frame-Options`,
    /// `Content-Security-Policy`, `X-XSS-Protection`) to every response. HSTS is
    /// intentionally omitted because the Python server runs over plaintext by
    /// default; set `with_hsts=True` only when terminating TLS in-process.
    #[pyo3(name = "enable_secure_headers")]
    fn enable_secure_headers(&mut self, with_hsts: Option<bool>) {
        self.secure_headers = true;
        if with_hsts.unwrap_or(false) {
            // HSTS included only on explicit opt-in for a TLS-terminating deploy.
            self.secure_headers_config =
                Some(justapi_core::middleware::SecurityHeaders::default().with_hsts_preload());
        }
    }

    /// Register a Python callable as a readiness probe. The callable is invoked
    /// synchronously under the GIL; a truthy return (or no exception) means the
    /// dependency is healthy. Used by the `/ready` builtin via the health
    /// registry. `name` identifies the component in the readiness report.
    #[pyo3(name = "register_health_check")]
    fn register_health_check(&mut self, name: String, check: Py<PyAny>) {
        self.health_checks.push((name, check));
    }

    /// Return the live Prometheus exposition for the running server. Used by the
    /// Python `/metrics` builtin so it exports real data instead of a stub.
    #[pyo3(name = "metrics_prometheus")]
    fn metrics_prometheus(&self) -> String {
        match &self.metrics {
            Some(m) => m.prometheus(),
            None => "# metrics not yet initialised\n".to_string(),
        }
    }

    /// Expose GraphQL gateway endpoint handling for Python app built-in route.
    #[pyo3(name = "graphql_handle")]
    fn graphql_handle(&self, method: String, body: Option<Vec<u8>>) -> (u16, String, String) {
        if method == "GET" {
            let source = justapi_core::graphql::graphiql_html();
            (200, "text/html; charset=utf-8".to_string(), source)
        } else {
            let body_bytes = body.unwrap_or_default();
            let schema = justapi_core::graphql::create_schema();
            let rt = CACHED_TOKIO_RT.get_or_init(|| {
                tokio::runtime::Builder::new_current_thread()
                    .enable_all()
                    .build()
                    .expect("failed to build cached tokio runtime")
            });
            let res_str = rt.block_on(async {
                justapi_core::graphql::execute_graphql_bytes(&schema, &body_bytes)
                    .await
                    .unwrap_or_else(|e| {
                        format!(r#"{{"errors":[{{"message":{:?}}}]}}"#, e.to_string())
                    })
            });
            (200, "application/json".to_string(), res_str)
        }
    }

    /// Returns `(ready, report_json)` for the running server's health registry.
    /// `ready` is true unless a registered dependency check is unhealthy. The
    /// Python `/ready` builtin uses this to give real readiness (not a static
    /// 200). When no dependencies are registered the app is always ready.
    #[pyo3(name = "health_ready")]
    fn health_ready(&self) -> (bool, String) {
        match &self.health_registry {
            Some(reg) => {
                let rt = CACHED_TOKIO_RT.get_or_init(|| {
                    tokio::runtime::Builder::new_current_thread()
                        .enable_all()
                        .build()
                        .expect("failed to build cached tokio runtime")
                });
                let report = rt.block_on(reg.check_all());
                let ready =
                    !matches!(report.overall(), justapi_core::health::OverallHealth::Unhealthy);
                let json = serde_json::to_string(&report)
                    .unwrap_or_else(|_| "{\"status\":\"unknown\"}".to_string());
                (ready, json)
            }
            None => (true, "{\"status\":\"ready\"}".to_string()),
        }
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

    /// Register a native MCP tool. `schema_json` is a JSON Schema (string)
    /// describing the tool's input. The `handler` is a Python callable invoked
    /// with the tool arguments as keyword arguments.
    #[pyo3(name = "register_tool")]
    fn register_tool(
        &mut self,
        name: String,
        description: String,
        schema_json: String,
        handler: Py<PyAny>,
    ) -> PyResult<()> {
        // Validate the schema parses as JSON up front.
        let _: serde_json::Value = serde_json::from_str(&schema_json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid tool schema: {e}"))
        })?;
        // Refuse duplicate tool names.
        if self.tools.iter().any(|t| t.name == name) {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "tool '{name}' is already registered"
            )));
        }
        self.tools.push(PyTool { name, description, schema: schema_json, handler });
        Ok(())
    }

    /// Return the registered tools in MCP `tools/list` shape (list of dicts with
    /// `name`, `description`, `inputSchema`).
    #[pyo3(name = "list_tools")]
    fn list_tools(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json_mod = PyModule::import(py, "json")?;
        let loads = json_mod.getattr("loads")?;
        let out = PyList::empty(py);
        for t in &self.tools {
            let schema: Py<PyAny> = loads.call1((t.schema.clone(),))?.into();
            let d = PyDict::new(py);
            d.set_item("name", t.name.clone())?;
            d.set_item("description", t.description.clone())?;
            d.set_item("inputSchema", schema)?;
            out.append(d)?;
        }
        Ok(out.into())
    }

    /// Invoke a registered tool by name with a JSON-encoded arguments object.
    /// Returns the tool handler's return value (a coroutine if the handler is
    /// async — the Python caller is responsible for awaiting it).
    #[pyo3(name = "call_tool")]
    fn call_tool(&self, py: Python<'_>, name: String, args_json: String) -> PyResult<Py<PyAny>> {
        let tool = self.tools.iter().find(|t| t.name == name).ok_or_else(|| {
            pyo3::exceptions::PyKeyError::new_err(format!("unknown tool '{name}'"))
        })?;
        let value: serde_json::Value = serde_json::from_str(&args_json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid tool args: {e}"))
        })?;
        let kwargs = PyDict::new(py);
        if let serde_json::Value::Object(map) = value {
            for (k, v) in map {
                kwargs.set_item(k, json_value_to_py(py, &v)?)?;
            }
        }
        let args = PyTuple::empty(py);
        let result = tool.handler.bind(py).call(args, Some(&kwargs))?;
        Ok(result.into())
    }

    // --- Agent session state -----------------------------------------------

    /// Create a new session, optionally seeded with `initial_json` (a JSON
    /// object). Returns the new session id.
    #[pyo3(name = "create_session")]
    fn create_session_rs(
        &self,
        id: Option<String>,
        initial_json: Option<String>,
    ) -> PyResult<String> {
        let value: serde_json::Value = match initial_json {
            Some(s) => serde_json::from_str(&s).map_err(|e| {
                pyo3::exceptions::PyValueError::new_err(format!("invalid session JSON: {e}"))
            })?,
            None => serde_json::Value::Object(serde_json::Map::new()),
        };
        let id = id.unwrap_or_else(|| Uuid::new_v4().simple().to_string());
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).insert(id.clone(), value);
        Ok(id)
    }

    /// Return a session's JSON value as a string, or `None` if unknown.
    #[pyo3(name = "get_session")]
    fn get_session_rs(&self, id: String) -> Option<String> {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).get(&id).map(|v| v.to_string())
    }

    /// Overwrite a session's value with `json`. Returns `false` if unknown.
    #[pyo3(name = "set_session")]
    fn set_session_rs(&self, id: String, json: String) -> PyResult<bool> {
        let value: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid session JSON: {e}"))
        })?;
        let mut store = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match store.entry(id) {
            std::collections::hash_map::Entry::Occupied(mut e) => {
                e.insert(value);
                Ok(true)
            }
            std::collections::hash_map::Entry::Vacant(_) => Ok(false),
        }
    }

    /// Merge `json` into an existing session (shallow merge of top-level
    /// object keys). Returns `false` if unknown.
    #[pyo3(name = "update_session")]
    fn update_session_rs(&self, id: String, json: String) -> PyResult<bool> {
        let incoming: serde_json::Value = serde_json::from_str(&json).map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("invalid session JSON: {e}"))
        })?;
        let mut store = self.sessions.lock().unwrap_or_else(|e| e.into_inner());
        match store.get_mut(&id) {
            None => Ok(false),
            Some(existing) => {
                if let serde_json::Value::Object(inc) = &incoming {
                    if let serde_json::Value::Object(ex) = existing {
                        for (k, v) in inc {
                            ex.insert(k.clone(), v.clone());
                        }
                        return Ok(true);
                    }
                }
                *existing = incoming;
                Ok(true)
            }
        }
    }

    /// Delete a session. Returns `true` if it existed.
    #[pyo3(name = "delete_session")]
    fn delete_session_rs(&self, id: String) -> bool {
        self.sessions.lock().unwrap_or_else(|e| e.into_inner()).remove(&id).is_some()
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, query_schema=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn get(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        query_schema: Option<Py<PyAny>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let key = (Method::GET, path.to_string());
        let is_native = native.unwrap_or(false);
        let crud_spec = make_crud_spec(crud_table, crud_columns)?;
        let query_json = resolve_schema_json(py, query_schema)?;

        if let Some(&id) = self.route_indices.get(&key) {
            self.handlers[id] = handler;
            self.native[id] = is_native;
            self.crud[id] = crud_spec;
            self.schemas[id] = None;
            self.schema_jsons[id] = None;
            self.query_schema_jsons[id] = query_json;
        } else {
            let id = self.handlers.len();
            self.handlers.push(handler);
            self.native.push(is_native);
            self.crud.push(crud_spec);
            self.sse_specs.push(None);
            self.schemas.push(None);
            self.schema_jsons.push(None);
            self.query_schema_jsons.push(query_json);
            self.batch_configs.push(None);
            self.route_indices.insert(key, id);
            self.router
                .insert(Method::GET, path, id)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        }
        self.store_meta(
            py,
            "GET".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, query_schema=None, batch_size=None, batch_window_ms=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn post(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        query_schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(body_schema.as_ref().map(|b| b.clone_ref(py)));
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.store_meta(
            py,
            "POST".to_string(),
            path.to_string(),
            body_schema,
            self.schema_jsons.last().cloned(),
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::POST, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, query_schema=None, batch_size=None, batch_window_ms=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn put(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        query_schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(body_schema.as_ref().map(|b| b.clone_ref(py)));
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.store_meta(
            py,
            "PUT".to_string(),
            path.to_string(),
            body_schema,
            self.schema_jsons.last().cloned(),
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::PUT, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, query_schema=None, batch_size=None, batch_window_ms=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn patch(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        query_schema: Option<Py<PyAny>>,
        batch_size: Option<usize>,
        batch_window_ms: Option<u64>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(body_schema.as_ref().map(|b| b.clone_ref(py)));
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(batch_size.map(|s| (s, batch_window_ms.unwrap_or(10))));
        self.store_meta(
            py,
            "PATCH".to_string(),
            path.to_string(),
            body_schema,
            self.schema_jsons.last().cloned(),
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::PATCH, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, query_schema=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn delete(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        query_schema: Option<Py<PyAny>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(None);
        self.store_meta(
            py,
            "DELETE".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::DELETE, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Register a route for the HTTP QUERY method (RFC 10008): safe,
    /// idempotent, and body-carrying (like POST). The `experimental` flag
    /// is reserved for tagging the operation in generated OpenAPI specs.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, body_schema=None, schema=None, query_schema=None, experimental=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn query(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        body_schema: Option<Py<PyAny>>,
        schema: Option<Py<PyAny>>,
        query_schema: Option<Py<PyAny>>,
        experimental: Option<bool>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let experimental = experimental.unwrap_or(false);
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(body_schema.as_ref().map(|b| b.clone_ref(py)));
        self.schema_jsons.push(resolve_schema_json(py, schema)?);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(None);
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.store_meta(
            py,
            justapi_core::query_method().as_str().to_string(),
            path.to_string(),
            body_schema,
            self.schema_jsons.last().cloned(),
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            experimental,
            include_in_schema,
        );
        self.router
            .insert(justapi_core::query_method(), path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Register a Rust-native SSE stream route (ADR-088): the server streams
    /// `count` events at `interval_ms` spacing, generated entirely in Rust
    /// (tokio + mpsc + streaming_response). No Python handler, no GIL, no
    /// pump — the streaming runs at native speed.
    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, count=10, interval_ms=100, tags=None, summary=None, description=None, deprecated=None, name=None, include_in_schema=None))]
    fn sse_native(
        &mut self,
        py: Python<'_>,
        path: &str,
        count: u64,
        interval_ms: u64,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        name: Option<String>,
        include_in_schema: Option<bool>,
    ) -> PyResult<()> {
        let key = (Method::GET, path.to_string());
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        // Placeholder handler — never invoked; the dispatch recognizes the
        // sse_spec and serves the Rust stream directly.
        let handler = py.None();
        if let Some(&id) = self.route_indices.get(&key) {
            self.handlers[id] = handler;
            self.sse_specs[id] = Some((count, interval_ms));
        } else {
            let id = self.handlers.len();
            self.handlers.push(handler);
            self.native.push(false);
            self.crud.push(None);
            self.sse_specs.push(Some((count, interval_ms)));
            self.schemas.push(None);
            self.schema_jsons.push(None);
            self.query_schema_jsons.push(None);
            self.batch_configs.push(None);
            self.route_indices.insert(key, id);
            self.router
                .insert(Method::GET, path, id)
                .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))?;
        }
        self.store_meta(
            py,
            "GET".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            None,
            None,
            None,
            None,
            false,
            Some(include_in_schema.unwrap_or(true)),
        );
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, query_schema=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn head(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        query_schema: Option<Py<PyAny>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(None);
        self.store_meta(
            py,
            "HEAD".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::HEAD, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, query_schema=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn options(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        query_schema: Option<Py<PyAny>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(None);
        self.store_meta(
            py,
            "OPTIONS".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::OPTIONS, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[allow(clippy::too_many_arguments)]
    #[pyo3(signature = (path, handler, query_schema=None, tags=None, summary=None, description=None, deprecated=None, status_code=None, responses=None, operation_id=None, openapi_extra=None, include_in_schema=None, name=None, native=None, crud_table=None, crud_columns=None))]
    fn trace(
        &mut self,
        py: Python<'_>,
        path: &str,
        handler: Py<PyAny>,
        query_schema: Option<Py<PyAny>>,
        tags: Option<Vec<String>>,
        summary: Option<String>,
        description: Option<String>,
        deprecated: Option<bool>,
        status_code: Option<u16>,
        responses: Option<String>,
        operation_id: Option<String>,
        openapi_extra: Option<String>,
        include_in_schema: Option<bool>,
        name: Option<String>,
        native: Option<bool>,
        crud_table: Option<String>,
        crud_columns: Option<Vec<String>>,
    ) -> PyResult<()> {
        let id = self.handlers.len();
        self.handlers.push(handler);
        self.native.push(native.unwrap_or(false));
        self.crud.push(make_crud_spec(crud_table, crud_columns)?);
        self.sse_specs.push(None);
        self.schemas.push(None);
        self.schema_jsons.push(None);
        self.query_schema_jsons.push(resolve_schema_json(py, query_schema)?);
        self.batch_configs.push(None);
        self.store_meta(
            py,
            "TRACE".to_string(),
            path.to_string(),
            None,
            None,
            tags,
            summary,
            description,
            deprecated,
            status_code,
            responses,
            operation_id,
            openapi_extra,
            false,
            include_in_schema,
        );
        if let Some(ref n) = name {
            self.named_routes.insert(n.clone(), path.to_string());
        }
        self.router
            .insert(Method::TRACE, path, id)
            .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    /// Set the database configuration.
    ///
    /// Accepts either a connection-string shorthand or a `dict` of tuning
    /// options (as produced by `Database.config_dict()`). The Python-side
    /// wrapper converts a `Database` object into that dict *before* crossing the
    /// FFI, because reading `Option`/`f64` fields off a `Database` instance from
    /// Rust hits a pyo3 `from_py_object` reconstruction quirk (values come back
    /// as defaults). A plain `dict` extracts reliably.
    #[pyo3(signature = (db))]
    fn set_database(&mut self, _py: Python<'_>, db: &Bound<'_, PyAny>) -> PyResult<()> {
        // A plain connection string is the common shorthand.
        if let Ok(url) = db.extract::<String>() {
            self.database = Some(Database::new(url, 10));
            return Ok(());
        }
        // Otherwise expect a `dict` of config options.
        let cfg: Py<PyDict> = match db.extract() {
            Ok(d) => d,
            Err(_) => {
                return Err(pyo3::exceptions::PyTypeError::new_err(
                    "database must be a connection string or a config dict",
                ))
            }
        };
        let cfg = cfg.bind(_py).as_any();
        let url: String = cfg.get_item("url")?.extract()?;
        let max_connections: u32 = cfg.get_item("max_connections")?.extract()?;
        let init_sql: Option<String> = cfg.get_item("init_sql")?.extract()?;
        let pragmas: Option<Vec<String>> = cfg.get_item("pragmas")?.extract()?;
        let acquire_timeout: Option<f64> = cfg.get_item("acquire_timeout")?.extract()?;
        let request_acquire_timeout: f64 = cfg.get_item("request_acquire_timeout")?.extract()?;
        let idle_timeout: Option<f64> = cfg.get_item("idle_timeout")?.extract()?;
        let max_lifetime: Option<f64> = cfg.get_item("max_lifetime")?.extract()?;
        let health_check_interval: Option<f64> =
            cfg.get_item("health_check_interval")?.extract()?;
        let isolation: Option<String> = cfg.get_item("isolation")?.extract()?;
        self.database = Some(Database {
            url,
            max_connections,
            init_sql,
            pragmas,
            acquire_timeout,
            request_acquire_timeout,
            idle_timeout,
            max_lifetime,
            health_check_interval,
            isolation,
        });
        Ok(())
    }

    /// Get the database configuration, or None.
    fn get_database(&self) -> Option<Database> {
        self.database.clone()
    }

    /// Connect the configured database pool eagerly, outside of `run()`, so that
    /// `app.db` is usable immediately after `app.set_database(...)` (e.g. from a
    /// REPL, a test, or a migration step) — not only once the server has started.
    ///
    /// This is idempotent: if the pool is already resolved it returns early. The
    /// DB round-trip runs with the GIL released on a throwaway tokio runtime.
    fn connect_database(&mut self, py: Python<'_>) -> PyResult<()> {
        if self.db_pool.is_some() {
            return Ok(());
        }
        let config = match self.database.as_ref() {
            Some(db) => db.to_config(),
            None => {
                return Err(pyo3::exceptions::PyValueError::new_err(
                    "no database configured; call app.set_database(url) first",
                ))
            }
        };
        // Use a MULTI-THREADED runtime for the DB pool. A current-thread
        // runtime's handle can only be driven from the thread that owns it;
        // calling `block_on` on it from another thread (e.g. a GIL-pool worker,
        // or the server runtime thread) deadlocks. A multi-thread runtime's
        // handle is safe to `block_on` from any thread — it temporarily drives
        // the runtime on the caller's thread — which is required for concurrent
        // DB-backed requests (see DECISIONS.md ADR-068, fixes D3/D4).
        let rt = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .worker_threads(4)
            .build()
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        let handle = rt.handle().clone();
        // Connect + bootstrap inside one GIL-released async block so no Python
        // token is alive during the DB round-trip. Returns an owned pool.
        let pool = py
            .detach(|| {
                handle.block_on(async {
                    let mut mgr = justapi_core::db::PoolManager::new();
                    let pool = mgr.init("", config.clone()).await.map_err(|e| e.to_string())?;
                    if let Some(sql) = &config.init_sql {
                        for stmt in sql.split(';') {
                            let stmt = stmt.trim();
                            if !stmt.is_empty() {
                                pool.execute(stmt).await.map_err(|e| e.to_string())?;
                            }
                        }
                    }
                    if let Some(interval) = config.health_check_interval {
                        let mgr = std::sync::Arc::new(mgr);
                        mgr.spawn_health_checks(interval).await;
                    }
                    Ok::<_, String>(pool)
                })
            })
            .map_err(|e: String| pyo3::exceptions::PyRuntimeError::new_err(e))?;
        // Keep the runtime alive for the app's lifetime so the DbPool's Handle
        // remains valid (e.g. for REPL/test use before `run()`).
        self.db_runtime = Some(rt);
        self.db_pool = Some(crate::database::DbPool::new(pool, handle));
        Ok(())
    }

    /// Return the resolved DB pool handle (`DbPool`) if a database was configured
    /// and the pool is connected (via `connect_database` or after `run()` starts),
    /// else `None`. Handlers use this to run arbitrary, injection-safe SQL in Rust
    /// (no GIL during the DB round-trip).
    fn db_pool(&self) -> Option<crate::database::DbPool> {
        self.db_pool.clone()
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

    /// Register a static frontend (SPA) served from `directory` under `path`.
    /// `fallback` is an optional file (e.g. "index.html") served for unknown
    /// routes; `None` disables SPA fallback. When `check_dir` is true the
    /// directory must already exist.
    #[pyo3(signature = (path, directory, fallback=None, check_dir=true))]
    fn frontend(
        &mut self,
        path: String,
        directory: String,
        fallback: Option<String>,
        check_dir: bool,
    ) -> PyResult<()> {
        if check_dir && !std::path::Path::new(&directory).is_dir() {
            return Err(pyo3::exceptions::PyValueError::new_err(format!(
                "frontend directory does not exist or is not a directory: {}",
                directory
            )));
        }
        let prefix = if path.is_empty() { "/".to_string() } else { path };
        let dir = justapi_core::static_files::StaticDir::new(directory);
        self.frontend_mounts.push(justapi_core::static_files::StaticMount {
            prefix,
            dir,
            fallback,
        });
        Ok(())
    }

    /// Build a URL path for a named route, substituting `{param}` placeholders
    /// from the provided keyword arguments. Mirrors FastAPI/Starlette
    /// `request.url_for(name, **params)`.
    #[pyo3(signature = (name, **kwargs))]
    fn url_for<'py>(
        &self,
        name: &str,
        kwargs: Option<&Bound<'py, PyDict>>,
        py: Python<'py>,
    ) -> PyResult<Py<PyAny>> {
        let template = self.named_routes.get(name).ok_or_else(|| {
            pyo3::exceptions::PyKeyError::new_err(format!("No route named {name:?} is registered"))
        })?;
        let mut params: std::collections::HashMap<String, String> =
            std::collections::HashMap::new();
        if let Some(d) = kwargs {
            for (k, v) in d.iter() {
                let key = k.str()?.to_string_lossy().to_string();
                let val = match v.extract::<String>() {
                    Ok(s) => s,
                    Err(_) => v.str()?.to_string_lossy().to_string(),
                };
                params.insert(key, val);
            }
        }
        let mut out = String::new();
        let mut chars = template.chars().peekable();
        while let Some(c) = chars.next() {
            if c == '{' {
                let mut key = String::new();
                for cc in chars.by_ref() {
                    if cc == '}' {
                        break;
                    }
                    key.push(cc);
                }
                match params.get(&key) {
                    Some(v) => out.push_str(v),
                    None => {
                        out.push('{');
                        out.push_str(&key);
                        out.push('}');
                    }
                }
            } else {
                out.push(c);
            }
        }
        Ok(PyString::new(py, &out).into_any().unbind())
    }

    /// Start the server and begin accepting requests.
    #[pyo3(signature = (addr, max_body_size=50*1024*1024))]
    fn run(slf: Py<Self>, py: Python<'_>, addr: &str, max_body_size: usize) -> PyResult<()> {
        let mut app = slf.borrow_mut(py);
        let addr: std::net::SocketAddr = addr.parse().map_err(|e| {
            pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {}", e))
        })?;

        // Ensure a default logger is installed before serving, so that the
        // server's `tracing` events (listen address, per-request spans,
        // connection errors, graceful shutdown) are actually surfaced. A user
        // who called `justapi.init_logging(...)` first keeps their config.
        justapi_core::tracing_setup::init_default_if_unset();

        // Initialize the dedicated GIL pool once (detects GIL vs free-threaded
        // mode automatically; see gil_pool.rs / DECISIONS.md ADR-049).
        crate::gil_pool::init(py, None);

        // Build OpenAPI spec from registered routes before taking ownership.
        let openapi_spec = {
            use justapi_core::openapi::*;
            let routes: Vec<(hyper::Method, String)> = app.router.list_routes().to_vec();
            // HEAD is auto-registered for GET routes; only surface an explicit
            // HEAD operation when no GET exists for the same path (FastAPI
            // does not list the implicit HEAD).
            let get_paths: std::collections::HashSet<&String> =
                routes.iter().filter(|(m, _)| *m == hyper::Method::GET).map(|(_, p)| p).collect();
            let mut registry = OpenApiRegistry::new();
            for (method, path) in &routes {
                if *method == hyper::Method::HEAD && get_paths.contains(path) {
                    continue;
                }
                let meta =
                    app.route_meta.get(&(method.clone(), path.clone())).cloned().unwrap_or_else(
                        || {
                            let is_body_method = matches!(
                                *method,
                                hyper::Method::POST | hyper::Method::PUT | hyper::Method::PATCH
                            ) || *method == justapi_core::query_method();
                            RouteMeta {
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
                                experimental: *method == justapi_core::query_method(),
                                status_code: None,
                                responses: None,
                                operation_id: None,
                                openapi_extra: None,
                                include_in_schema: true,
                            }
                        },
                    );
                registry.register(meta);
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

        let router = Arc::new(std::mem::take(&mut app.router));
        let handlers = Arc::new(std::mem::take(&mut app.handlers));
        // Per-handler flag: does this route need a Python `Request` object?
        // 0-param, dependency-free handlers skip building it (mirrors Robyn's
        // `number_of_params == 0 -> call0()` fast path).
        let needs_request: Vec<bool> = handlers
            .iter()
            .map(|h| {
                h.bind(py)
                    .getattr("_needs_request")
                    .and_then(|v| v.extract::<bool>())
                    .unwrap_or(true)
            })
            .collect();
        let needs_request = Arc::new(needs_request);
        // Per-handler flag: `@native_async` — async handler whose framework
        // ops run in Rust; dispatch via the fastest async path (ADR-089).
        let native_async: Vec<bool> = handlers
            .iter()
            .map(|h| {
                h.bind(py)
                    .getattr("_is_native_async")
                    .and_then(|v| v.extract::<bool>())
                    .unwrap_or(false)
            })
            .collect();
        let native_async = Arc::new(native_async);
        // Per-handler flag: is this route served by the native Rust fast path
        // (schema-backed, response = validated request body, no Python call)?
        let native: Vec<bool> = std::mem::take(&mut app.native);
        let native = Arc::new(native);
        // Per-handler Rust-native CRUD config (ADR-056 Step C). `Some((table,
        // columns, id_column))` means the route is served by `crud_dispatch_bytes`
        // with the operation inferred from the request method. The pool is
        // resolved at request time from `db_pool`.
        let crud_specs: crate::native::handlers::CrudConfig =
            Arc::new(std::mem::take(&mut app.crud));
        let sse_specs: Arc<Vec<Option<(u64, u64)>>> = Arc::new(std::mem::take(&mut app.sse_specs));
        let schemas = Arc::new(std::mem::take(&mut app.schemas));
        let schema_jsons = Arc::new(std::mem::take(&mut app.schema_jsons));
        let query_schema_jsons = Arc::new(std::mem::take(&mut app.query_schema_jsons));
        // Precompile each schema once (per route) so validation on the hot path
        // reuses the compiled validator instead of re-parsing/re-compiling the
        // schema JSON on every request. A compile failure (shouldn't happen for
        // a schema already accepted at registration) leaves `None`, and the
        // route falls back to the raw-schema-string validation path.
        let schema_validators: Vec<Option<justapi_core::validate::CompiledValidator>> =
            vec![None; schema_jsons.len()];
        let schema_validators = Arc::new(schema_validators);
        let batch_configs_arc = Arc::new(std::mem::take(&mut app.batch_configs));
        let database_config = app.database.as_ref().map(|d| d.to_config());

        // Eagerly resolve the pool now (if configured and not already connected)
        // so the rest of `run` and any handler sees a ready `app.db`. `connect_database`
        // reads `self.database`, so we keep it (no `.take()`).
        if database_config.is_some() && app.db_pool.is_none() {
            let connect_err = match app.connect_database(py) {
                Ok(()) => None,
                Err(e) => Some(e.to_string()),
            };
            if let Some(msg) = connect_err {
                return Err(pyo3::exceptions::PyRuntimeError::new_err(format!(
                    "Database init error: {}",
                    msg
                )));
            }
        }
        // Capture the (now resolved) pool before the borrow is released, so the
        // detached server loop — which must be `Send` — can use it without moving
        // the non-`Send` `PyRefMut`.
        let db_pool = app.db_pool.clone();
        let plugins = std::mem::take(&mut app.plugins);
        let grpc_addr = app.grpc_addr.take();
        let grpc_handlers = Arc::new(std::mem::take(&mut app.grpc_handlers));
        let ws_routes = std::mem::take(&mut app.ws_routes);
        let wasm_bytes = app.wasm_bytes.take();
        let gateway_config_path = app.gateway_config.take();
        let cb_config = app.circuit_breaker_config.take();
        let coalesce_headers = app.coalesce_headers.take();
        let health_checks = std::mem::take(&mut app.health_checks);
        let secure_headers = app.secure_headers;
        let secure_headers_config = app.secure_headers_config.take();
        let frontend_mounts = std::mem::take(&mut app.frontend_mounts);
        let jwt_auth = app.jwt_auth.take();
        let cors = app.cors.take();
        #[cfg(feature = "http3")]
        let (app_http3_cert, app_http3_key) = (app.http3_cert.clone(), app.http3_key.clone());

        // The app object is surfaced on `Request.app`; capture it before
        // releasing the borrow (the server loop runs without the GIL held).
        let app_py: Option<Py<PyAny>> = Some(slf.clone_ref(py).into_bound(py).into_any().unbind());
        drop(app);
        // Clone the app handle now (GIL held) so it can be moved into the
        // detached server loop, which runs without a GIL token.
        let app_py_http1: Option<Py<PyAny>> = app_py.as_ref().map(|a| a.clone_ref(py));
        #[cfg(feature = "http3")]
        let app_py_http3: Arc<Option<Py<PyAny>>> =
            Arc::new(app_py_http1.as_ref().map(|a| a.clone_ref(py)));
        let app_py_ws: Arc<Option<Py<PyAny>>> =
            Arc::new(app_py_http1.as_ref().map(|a| a.clone_ref(py)));
        let _ = app_py;

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
            // Multi-threaded runtime: the HTTP server (accept loop, per-
            // connection tasks, TLS, response streaming) must not run on a
            // single thread. `Runtime::new()` is current-thread — one thread
            // handles ALL I/O, capping throughput and making the server
            // fragile under load spikes. Match the DB pool's 4-worker
            // multi-threaded runtime (ADR-068).
            let rt = tokio::runtime::Builder::new_multi_thread()
                .enable_all()
                .worker_threads(
                    std::env::var("JUSTAPI_SERVER_THREADS")
                        .ok()
                        .and_then(|v| v.parse::<usize>().ok())
                        .filter(|n| *n > 0)
                        .unwrap_or_else(|| {
                            std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
                        }),
                )
                .build()?;
            rt.block_on(async {
                // `db_pool` is the resolved `DbPool` (already connected), ready
                // for handlers and exposed to Python.
                let db_url_str = database_config.as_ref().map(|c| c.url.clone());

                // A Rust-native CRUD route (ADR-056) needs a database pool; fail
                // loudly rather than silently falling back to the Python path.
                if crud_specs.iter().any(|c| c.is_some()) && db_pool.is_none() {
                    return Err(anyhow::anyhow!(
                        "a route registered with crud_table/crud_columns requires a database connection (set app.database)"
                    ));
                }

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
                                    crate::gil_pool::run_python(move |py| {
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
                                                "http".to_string(),
                                                None,
                                                None,
                                                "1.1".to_string(),
                                                None, // auth_claims: batched requests have no middleware
                                            )).unwrap_or_else(|e| {
                                                tracing::error!("Failed to create batch Request object: {e}");
                                                panic!("Fatal: unable to create batch Request object: {e}")
                                            });
                                            py_list.append(r).unwrap_or_else(|e| {
                                                tracing::error!("Failed to append batch request to list: {e}");
                                            });
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
                // Clone the Arc inputs so the HTTP/3 handler (built below,
                // same app state) can share them after the TCP handler takes
                // the originals.
                #[cfg(feature = "http3")]
                let (h3_router, h3_handlers, h3_schemas, h3_schema_jsons, h3_batchers, h3_db_url) =
                    (router.clone(), handlers.clone(), schemas.clone(), schema_jsons.clone(), batchers.clone(), db_url_str.clone());
                let handler = make_native_handler(
                    router,
                    handlers,
                    schemas,
                    schema_jsons,
                    query_schema_jsons.clone(),
                    batchers,
                    db_pool.as_ref().map(|p| p.as_any_pool()),
                    db_url_str,
                    "http".to_string(),
                    app_py_http1,
                    needs_request.clone(),
                    native.clone(),
                    native_async.clone(),
                    crud_specs.clone(),
                    sse_specs.clone(),
                    schema_validators.clone(),
                    max_body_size,
                );

                // HTTP/3 (QUIC): build a second handler chain over
                // `Full<Bytes>` bodies from the same app state and serve it on
                // UDP alongside the TCP server. QUIC requires TLS certs; see
                // `enable_http3`. The TCP chain and the QUIC chain share the
                // same route table and Python handlers. (Spawn happens right
                // before `server.run()` below.)
                #[cfg(feature = "http3")]
                let http3_bridge: Option<(
                    justapi_core::http3::Http3Config,
                    justapi_core::http3::Http3Handler,
                )> = if let (Some(cert), Some(key)) =
                    (app_http3_cert.clone(), app_http3_key.clone())
                {
                    let h3_query_jsons = query_schema_jsons.clone();
                    let h3_db = db_pool.as_ref().map(|p| p.as_any_pool());
                    let h3_app: Option<Py<PyAny>> =
                        Python::attach(|py| app_py_http3.as_ref().as_ref().map(|a| a.clone_ref(py)));
                    let h3_needs = needs_request.clone();
                    let h3_native = native.clone();
                    let h3_native_async = native_async.clone();
                    let h3_crud = crud_specs.clone();
                    let h3_sse = sse_specs.clone();
                    let h3_validators = schema_validators.clone();
                    let h3_handler: justapi_core::middleware::HandlerFn<
                        http_body_util::Full<bytes::Bytes>,
                    > = make_native_handler(
                        h3_router,
                        h3_handlers,
                        h3_schemas,
                        h3_schema_jsons,
                        h3_query_jsons,
                        h3_batchers,
                        h3_db,
                        h3_db_url,
                        "https".to_string(),
                        h3_app,
                        h3_needs,
                        h3_native,
                        h3_native_async,
                        h3_crud,
                        h3_sse,
                        h3_validators,
                        max_body_size,
                    );
                    let h3_chain = justapi_core::middleware::MiddlewareChain::new(h3_handler);
                    Some((
                        justapi_core::http3::Http3Config { cert_path: cert, key_path: key },
                        justapi_core::http3::chain_to_http3_handler(h3_chain),
                    ))
                } else {
                    None
                };

                // Install signal handlers to trigger graceful shutdown.
                // Ctrl+C is universal; on Unix we also handle SIGTERM so the
                // process can be cleanly stopped by container orchestrators
                // (Docker/k8s) and process managers.
                let signal_token = shutdown.clone();
                tokio::spawn(async move {
                    #[cfg(unix)]
                    {
                        let term = term_signal();
                        tokio::select! {
                            _ = tokio::signal::ctrl_c() => {
                                tracing::info!("Ctrl+C received, initiating graceful shutdown");
                                signal_token.cancel();
                            }
                            _ = term => {
                                tracing::info!("SIGTERM received, initiating graceful shutdown");
                                signal_token.cancel();
                            }
                        }
                    }
                    #[cfg(not(unix))]
                    {
                        tokio::signal::ctrl_c().await.ok();
                        tracing::info!("Ctrl+C received, initiating graceful shutdown");
                        signal_token.cancel();
                    }
                });

                let mut server = justapi_core::Server::new(addr)
                    .with_handler(handler)
                    .with_shutdown(shutdown_signal);
                if !health_checks.is_empty() {
                    let mut reg = justapi_core::health::HealthRegistry::new();
                    for (name, check) in health_checks {
                        reg.register(PyHealthCheck {
                            name: Box::leak(name.into_boxed_str()),
                            check,
                        });
                    }
                    server = server.with_health_registry(reg);
                }
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

                if secure_headers {
                    let sh = secure_headers_config
                        .unwrap_or_else(|| justapi_core::middleware::SecurityHeaders::default().without_hsts());
                    server = server.add_security_headers(sh);
                }

                // Wire JWT middleware into the Rust middleware chain. The decoded
                // claims are stored in `hyper::Request` extensions by the middleware
                // and bridged into `Request.state["auth"]` in the handler dispatch.
                if let Some(jwt) = jwt_auth {
                    tracing::info!("JWT authentication middleware enabled");
                    server = server.add_jwt(jwt);
                }

                // Wire CORS middleware into the Rust middleware chain. When
                // configured, this handles OPTIONS preflight (returning 204)
                // and injects `Access-Control-*` headers on every response.
                if let Some(cors) = cors {
                    tracing::info!("CORS middleware enabled");
                    server = server.add_cors(cors);
                }

                if let Some(path) = gateway_config_path {
                    tracing::info!("Starting Gateway with hot reloading for config: {}", path);
                    let gateway_state = justapi_core::gateway::GatewayState::new(&path);
                    if let Err(e) = gateway_state.clone().watch() {
                        tracing::error!("Failed to start gateway config file watcher: {}", e);
                    }
                    server = server.add_gateway(gateway_state);
                }

                if !frontend_mounts.is_empty() {
                    server = server.with_static_mounts(frontend_mounts.clone());
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
                    let ws_handler: justapi_core::server::WsHandler = Arc::new(move |info, mut read, mut write| {
                        let ws_routes = ws_routes_arc.clone();
                        let app_py_ws = app_py_ws.clone();
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
                                            let _ = in_tx.send(WsMessage::Close(None, None));
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
                                        WsMessage::Close(code, reason) => {
                                            tokio_tungstenite::tungstenite::Message::Close(
                                                code.map(|c| CloseFrame {
                                                    code: c.into(),
                                                    reason: reason
                                                        .clone()
                                                        .unwrap_or_default()
                                                        .into(),
                                                }),
                                            )
                                        }
                                    };
                                    if write.send(frame).await.is_err() {
                                        break;
                                    }
                                    if matches!(msg, WsMessage::Close(..)) {
                                        break;
                                    }
                                }
                                let _ = write.close().await;
                            });

                            // Invoke the Python handler on the daemon event loop.
                            Python::attach(|py| {
                                    if let Some(h) = ws_routes.get(&info.path) {
                                        let conn = Conn {
                                            method: "GET".to_string(),
                                            path: info.path.clone(),
                                            path_params_raw: vec![],
                                            query_string_raw: info.query_string.clone(),
                                            headers_raw: info.headers.clone(),
                                            scheme: "ws".to_string(),
                                            client: info.client.clone(),
                                            app: app_py_ws.as_ref().as_ref().map(|a| a.clone_ref(py)),
                                            http_version: "1.1".to_string(),
                                            state: PyDict::new(py).unbind(),
                                        };
                                    let rust_ws = match Py::new(
                                        py,
                                        WebSocket::new(incoming, out_tx, conn),
                                    ) {
                                        Ok(w) => w,
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to build Rust WebSocket: {}",
                                                e
                                            );
                                            return;
                                        }
                                    };
                                    let ws_obj = match PyModule::import(py, "justapi.websockets")
                                        .and_then(|m| m.getattr("WebSocket"))
                                        .and_then(|cls| cls.call1((rust_ws,)))
                                    {
                                        Ok(obj) => obj,
                                        Err(e) => {
                                            tracing::error!(
                                                "Failed to build Python WebSocket wrapper: {}",
                                                e
                                            );
                                            return;
                                        }
                                    };
                                    match h.bind(py).call1((ws_obj,)) {
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

                // Publish the live metrics collector onto the app object so the
                // Python `/metrics` builtin exports real data. `app` is a
                // `!Send` borrow, so we mutate through the owning `Py<Self>`
                // via `Python::attach` rather than capturing `app` here.
                let live_metrics = server.metrics().clone();
                let live_health = server.health_registry_arc();
                Python::attach(move |py| {
                    let mut a = slf.borrow_mut(py);
                    a.metrics = Some(live_metrics);
                    a.health_registry = Some(live_health);
                });

                // Start the HTTP/3 (QUIC) listener on the same address (UDP)
                // once the TCP server is fully configured.
                #[cfg(feature = "http3")]
                if let Some((h3_cfg, h3_bridge)) = http3_bridge {
                    let h3_metrics = server.metrics().clone();
                    let h3_cancel = shutdown.clone();
                    let h3_addr = addr;
                    tokio::spawn(async move {
                        match justapi_core::http3::serve_http3(
                            h3_addr,
                            h3_cfg,
                            h3_bridge,
                            h3_metrics,
                            h3_cancel,
                        )
                        .await
                        {
                            Ok(bound) => tracing::info!("HTTP/3 listening on udp://{}", bound),
                            Err(e) => tracing::error!("Failed to start HTTP/3 listener: {:#}", e),
                        }
                    });
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
