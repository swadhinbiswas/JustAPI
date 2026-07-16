use std::collections::HashMap;
use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::{Method, Request, Response, StatusCode};
use pyo3::prelude::*;
use pyo3::types::{PyBool, PyFloat, PyInt};
use serde_urlencoded;

use justapi_core::router::Router;
use justapi_core::{json_response, ResponseBody};

use justapi_core::db::AnyPool;

use super::types::*;
#[allow(clippy::too_many_arguments)]
/// Env-gated profiler for the GIL-path FFI cost. Activated only when the
/// `JUSTAPI_PROFILE` environment variable is set; otherwise returns immediately
/// after a cheap `OnceLock` boolean load, so it adds no measurable overhead to
/// the production hot path. Accumulates request-build and handler-call
/// nanoseconds and prints a rolling average every 100k requests.
fn profile_gil_path(build_ns: u64, handler_ns: u64) {
    fn enabled() -> bool {
        static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
        *ENABLED.get_or_init(|| std::env::var_os("JUSTAPI_PROFILE").is_some())
    }
    if !enabled() {
        return;
    }
    use std::sync::Mutex;
    static STATS: std::sync::OnceLock<Mutex<(u64, u128, u128)>> = std::sync::OnceLock::new();
    let mut s = STATS.get_or_init(|| Mutex::new((0u64, 0u128, 0u128))).lock().unwrap();
    s.0 += 1;
    s.1 += build_ns as u128;
    s.2 += handler_ns as u128;
    if s.0.is_multiple_of(100_000) {
        eprintln!(
            "[profile] n={} avg_request_build={}ns avg_handler={}ns",
            s.0,
            (s.1 / s.0 as u128) as u64,
            (s.2 / s.0 as u128) as u64,
        );
    }
}

#[allow(clippy::too_many_arguments)]
pub(crate) fn call_python_handler(
    py: Python<'_>,
    handler: &Py<PyAny>,
    schema: Option<&Py<PyAny>>,
    path_params: &[(String, String)],
    query_string: &[u8],
    headers: &[(Vec<u8>, Vec<u8>)],
    body: &[u8],
    db_url: Option<&str>,
    trace_ctx: Option<(String, String)>,
    multipart_form: Option<Py<pyo3::types::PyDict>>,
    method: &str,
    path: &str,
    scheme: String,
    client: Option<(String, u16)>,
    app: Option<Py<PyAny>>,
    needs_request: bool,
    http_version: String,
) -> NativeResponse {
    let helper = get_helper(py);

    // Set trace context in Python contextvars for distributed tracing. Gated
    // behind JUSTAPI_ENABLE_TRACE (default off) so the hot path stays cheap.
    if trace_enabled() {
        if let Some((trace_id, span_id)) = trace_ctx {
            let _ = helper.set_trace_context.bind(py).call1((trace_id, span_id));
        }
    }

    // Validate body if a schema is registered
    if let Some(schema) = schema {
        let validation_result = helper.validate_body.bind(py).call1((schema, body));
        match validation_result {
            Ok(errors) => {
                if let Ok(error_list) = errors.extract::<Vec<String>>() {
                    if !error_list.is_empty() {
                        let error_body = serde_json::json!({
                            "detail": error_list.join("; ")
                        })
                        .to_string();
                        return NativeResponse {
                            status: 422,
                            headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
                            body: NativeBody::Bytes(error_body.into_bytes()),
                        };
                    }
                }
            }
            Err(e) => {
                tracing::error!("Body validation error: {}", e);
            }
        }
    }

    let _tb0 = std::time::Instant::now();
    let request: Bound<'_, PyAny> = if needs_request {
        Bound::new(
            py,
            crate::request::Request::new(
                py,
                method.to_string(),
                path.to_string(),
                path_params.to_vec(),
                query_string.to_vec(),
                headers.to_vec(),
                body.to_vec(),
                db_url.map(|s| s.to_string()),
                multipart_form,
                scheme,
                client,
                app,
                http_version,
            ),
        )
        .unwrap()
        .into_any()
    } else {
        // Handlers with no request-dependent parameters (0-param endpoints,
        // no dependencies/query/path/etc.) skip building the Python `Request`
        // object entirely — it is never consulted.
        pyo3::types::PyDict::new(py).into_any()
    };
    let _build_ns = _tb0.elapsed().as_nanos() as u64;

    let _th0 = std::time::Instant::now();
    let result = helper.call_handler.bind(py).call1((handler, request));
    let _handler_ns = _th0.elapsed().as_nanos() as u64;
    profile_gil_path(_build_ns, _handler_ns);

    let final_res = match result {
        Ok(res) => {
            let is_future = res.hasattr("result").unwrap_or(false)
                && !res.is_instance_of::<pyo3::types::PyDict>();
            if is_future {
                res.call_method0("result")
            } else {
                Ok(res)
            }
        }
        Err(e) => Err(e),
    };

    match final_res {
        Ok(res) => {
            if let Ok(vs_res) =
                res.extract::<pyo3::PyRef<'_, super::app::ValidatedStreamResponse>>()
            {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let sender = StreamSender { tx };
                let pump = helper.pump_validated_stream.bind(py);
                if let Err(e) = pump.call1((
                    vs_res.generator.clone_ref(py),
                    vs_res.schema_json.clone(),
                    sender,
                    vs_res.mode.clone(),
                )) {
                    tracing::error!("Failed to pump validated stream: {}", e);
                }
                return NativeResponse {
                    status: vs_res.status,
                    headers: vs_res.headers.clone(),
                    body: NativeBody::Stream(rx),
                };
            }
            if let Ok(ts_res) = res.extract::<pyo3::PyRef<'_, TokenStreamResponse>>() {
                let (tx, rx) = tokio::sync::mpsc::unbounded_channel();
                let sender = StreamSender { tx };
                let pump = helper.pump_stream.bind(py);
                if let Err(e) = pump.call1((ts_res.generator.clone_ref(py), sender)) {
                    tracing::error!("Failed to pump stream: {}", e);
                }
                return NativeResponse {
                    status: ts_res.status,
                    headers: ts_res.headers.clone(),
                    body: NativeBody::Stream(rx),
                };
            }

            // Streaming responses are serialized by pumping the generator.
            // Everything else goes through the Rust-side fast serializer,
            // which mirrors Robyn's `extract_response_type_fast` (orjson
            // directly from Rust for dict/list/scalars, downcast for
            // `Response`, Python `wrap_result` only as a last resort).
            serialize_response(py, res, helper)
        }
        Err(e) => {
            // FastAPI-style exceptions: render as proper JSON error responses
            // instead of a generic 500.
            let justapi = py.import("justapi").ok();
            let val = e.value(py);
            if let Some(exc_cls) = justapi.as_ref().and_then(|m| m.getattr("HTTPException").ok()) {
                if val.is_instance(&exc_cls).unwrap_or(false) {
                    let status: u16 = val
                        .getattr("status_code")
                        .ok()
                        .and_then(|v| v.extract().ok())
                        .unwrap_or(500);
                    let detail = exception_detail(py, val);
                    let user_headers: Vec<(Vec<u8>, Vec<u8>)> = val
                        .getattr("headers")
                        .ok()
                        .and_then(|v| v.extract::<Option<HashMap<String, String>>>().ok())
                        .flatten()
                        .map(|m| {
                            m.into_iter().map(|(k, v)| (k.into_bytes(), v.into_bytes())).collect()
                        })
                        .unwrap_or_default();
                    let mut headers = user_headers;
                    headers.push((b"content-type".to_vec(), b"application/json".to_vec()));
                    let body = serde_json::json!({ "detail": detail }).to_string().into_bytes();
                    return NativeResponse { status, headers, body: NativeBody::Bytes(body) };
                }
            }
            if let Some(exc_cls) =
                justapi.as_ref().and_then(|m| m.getattr("RequestValidationError").ok())
            {
                if val.is_instance(&exc_cls).unwrap_or(false) {
                    let detail = exception_detail(py, val);
                    let headers = vec![(b"content-type".to_vec(), b"application/json".to_vec())];
                    let body = serde_json::json!({ "detail": detail }).to_string().into_bytes();
                    return NativeResponse { status: 422, headers, body: NativeBody::Bytes(body) };
                }
            }
            // Exception surfaced from the Python handler (or the call wrapper).
            // Log the detail server-side only; never leak it to the client.
            tracing::error!("Native handler error: {}", e);
            NativeResponse {
                status: 500,
                headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
                body: NativeBody::Bytes(b"{\"detail\":\"Internal Server Error\"}".to_vec()),
            }
        }
    }
}

/// Whether distributed-tracing context is injected into Python contextvars on
/// every request. Off by default; opt in with `JUSTAPI_ENABLE_TRACE=1` (or
/// `true`) so the hot path stays cheap.
fn trace_enabled() -> bool {
    static ENABLED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *ENABLED.get_or_init(|| match std::env::var("JUSTAPI_ENABLE_TRACE") {
        Ok(v) => v == "1" || v.eq_ignore_ascii_case("true"),
        Err(_) => false,
    })
}

/// Cached `orjson.dumps(obj, default=str)` (or `json.dumps` fallback) callable,
/// resolved once on first use. Mirrors `_native_helper._dumps` but lets Rust
/// invoke orjson directly, skipping the Python `wrap_result` bytecode.
pub(crate) fn fast_dumps(py: Python<'_>) -> PyResult<&Bound<'_, PyAny>> {
    static DUMPS: std::sync::OnceLock<Py<PyAny>> = std::sync::OnceLock::new();
    let expr = if py.import("orjson").is_ok() {
        "lambda o: __import__('orjson').dumps(o, default=str)"
    } else {
        "lambda o: __import__('json').dumps(o, default=str).encode('utf-8')"
    };
    // SAFETY: globals/locals are left as None, which Python fills with the
    // builtins dict, so `__import__` and `str` are available. The cached
    // lambda borrows no local state and is leaked for the process lifetime.
    let cexpr = std::ffi::CString::new(expr).unwrap();
    let dumps = DUMPS.get_or_init(|| py.eval(cexpr.as_c_str(), None, None).unwrap().unbind());
    Ok(dumps.bind(py))
}

/// Convert a raw Python handler return value into a `NativeResponse` using the
/// fastest path available, mirroring Robyn's `extract_response_type_fast`:
/// downcast a `Response` pyclass, serialize dict/list/scalars via orjson
/// directly from Rust, and only fall back to the Python `wrap_result` for
/// exotic types.
pub(crate) fn serialize_response(
    py: Python<'_>,
    res: Bound<'_, PyAny>,
    helper: &HelperFunctions,
) -> NativeResponse {
    // 1. `Response` (detected via a marker attribute set by the Python
    //    `Response` class) -> read fields directly, zero `to_dict` round-trip.
    //    Mirrors the original `wrap_result` behaviour, including running any
    //    attached background tasks.
    if res.getattr("_justapi_response").is_ok() {
        if let Ok(bg) = res.getattr("background") {
            if !bg.is_none() {
                if let Ok(run) = bg.getattr("run") {
                    let _ = run.call0();
                }
            }
        }
        let status: u16 =
            res.getattr("status_code").ok().and_then(|v| v.extract::<u16>().ok()).unwrap_or(200);
        let headers: Vec<(Vec<u8>, Vec<u8>)> = res
            .getattr("headers")
            .ok()
            .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>().ok())
            .unwrap_or_default();
        let body: Vec<u8> = match res.getattr("body") {
            Ok(b) => {
                if b.is_instance_of::<pyo3::types::PyBytes>() {
                    b.extract::<Vec<u8>>().unwrap_or_default()
                } else if let Ok(s) = b.extract::<String>() {
                    s.into_bytes()
                } else {
                    Vec::new()
                }
            }
            Err(_) => Vec::new(),
        };
        return NativeResponse { status, headers, body: NativeBody::Bytes(body) };
    }

    // 2. Legacy response dict (`{"status": ..., "body": ...}`) -> passthrough.
    if res.is_instance_of::<pyo3::types::PyDict>() {
        let has_body = res.get_item("body").is_ok();
        let has_status = res.get_item("status").is_ok();
        if has_body || has_status {
            let status: u16 =
                res.get_item("status").ok().and_then(|v| v.extract::<u16>().ok()).unwrap_or(200);
            let headers: Vec<(Vec<u8>, Vec<u8>)> = res
                .get_item("headers")
                .ok()
                .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>().ok())
                .unwrap_or_default();
            let body: Vec<u8> = match res.get_item("body").ok() {
                Some(b) => {
                    if let Ok(s) = b.extract::<String>() {
                        s.into_bytes()
                    } else if let Ok(bt) = b.extract::<Vec<u8>>() {
                        bt
                    } else {
                        b.str().map(|s| s.to_string().into_bytes()).unwrap_or_default()
                    }
                }
                None => Vec::new(),
            };
            return NativeResponse { status, headers, body: NativeBody::Bytes(body) };
        }
    }

    // 3. dict / list / str / number -> serialize via orjson directly from Rust.
    let is_json = res.is_instance_of::<pyo3::types::PyDict>()
        || res.is_instance_of::<pyo3::types::PyList>()
        || res.is_instance_of::<pyo3::types::PyString>()
        || res.is_instance_of::<PyInt>()
        || res.is_instance_of::<PyFloat>()
        || res.is_instance_of::<PyBool>()
        || res.is_none();
    if is_json {
        if let Ok(dumps) = fast_dumps(py) {
            if let Ok(out) = dumps.call1((res.clone(),)) {
                let body = if let Ok(b) = out.extract::<Vec<u8>>() {
                    b
                } else if let Ok(s) = out.extract::<String>() {
                    s.into_bytes()
                } else {
                    Vec::new()
                };
                return NativeResponse {
                    status: 200,
                    headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
                    body: NativeBody::Bytes(body),
                };
            }
        }
    }

    // 4. bytes -> application/octet-stream.
    if res.is_instance_of::<pyo3::types::PyBytes>() {
        if let Ok(body) = res.extract::<Vec<u8>>() {
            return NativeResponse {
                status: 200,
                headers: vec![(b"content-type".to_vec(), b"application/octet-stream".to_vec())],
                body: NativeBody::Bytes(body),
            };
        }
    }

    // 5. Slow-path fallback: exotic types go through the Python `wrap_result`,
    //    preserving all existing behaviour.
    if let Ok(wrapped) = helper.wrap_result.bind(py).call1((res,)) {
        let status: u16 =
            wrapped.get_item("status").ok().and_then(|v| v.extract::<u16>().ok()).unwrap_or(200);
        let headers: Vec<(Vec<u8>, Vec<u8>)> = wrapped
            .get_item("headers")
            .ok()
            .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>().ok())
            .unwrap_or_default();
        let body: Vec<u8> = wrapped
            .get_item("body")
            .ok()
            .and_then(|v| v.extract::<Vec<u8>>().ok())
            .unwrap_or_default();
        return NativeResponse { status, headers, body: NativeBody::Bytes(body) };
    }

    NativeResponse {
        status: 500,
        headers: vec![(b"content-type".to_vec(), b"text/plain".to_vec())],
        body: NativeBody::Bytes(b"Internal Server Error: failed to serialize response".to_vec()),
    }
}

pub(crate) fn nr_to_response(nr: NativeResponse) -> hyper::Response<ResponseBody> {
    let mut resp = match nr.body {
        NativeBody::Bytes(b) => {
            let mut r = Response::new(ResponseBody::new(
                http_body_util::Full::new(Bytes::from(b))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ));
            *r.status_mut() =
                StatusCode::from_u16(nr.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR);
            r
        }
        NativeBody::Stream(rx) => {
            let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(rx);
            justapi_core::streaming_response(
                StatusCode::from_u16(nr.status).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR),
                "text/event-stream",
                stream,
            )
        }
    };

    for (name, value) in &nr.headers {
        if let (Ok(n), Ok(v)) =
            (http::HeaderName::from_bytes(name), http::HeaderValue::from_bytes(value))
        {
            resp.headers_mut().insert(n, v);
        }
    }
    resp
}

/// Native fast path for schema-backed routes (`native=True`).
///
/// If the route is registered `native=True` and has a schema, the request body
/// is validated **entirely in Rust** and, on success, echoed back as the
/// response — no blocking-pool thread hop, no GIL acquisition, no `Request`
/// construction, and no Python handler call. This is the single biggest
/// optimization available for schema-backed routes.
///
/// Returns `Some(response)` when the native path handles the request, or `None`
/// when it should fall through to the normal Python handler (e.g. `native=False`
/// or `native=True` with no schema).
pub(crate) fn try_native_fast_path(
    handler_id: usize,
    native: &[bool],
    validators: &[Option<justapi_core::validate::CompiledValidator>],
    schema_jsons: &[Option<String>],
    body: &[u8],
) -> Option<NativeResponse> {
    if !native.get(handler_id).copied().unwrap_or(false) {
        return None;
    }
    let result = match validators.get(handler_id).and_then(|v| v.as_ref()) {
        Some(v) => v.validate(body),
        None => match schema_jsons.get(handler_id).and_then(|s| s.as_ref()) {
            Some(sj) => justapi_core::validate::validate_json_schema(body, sj),
            None => return None,
        },
    };
    match result {
        Ok(()) => Some(NativeResponse {
            status: 200,
            headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
            body: NativeBody::Bytes(body.to_vec()),
        }),
        Err(verr) => {
            let error_body = serde_json::json!({ "detail": verr.to_string() }).to_string();
            Some(NativeResponse {
                status: 422,
                headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
                body: NativeBody::Bytes(error_body.into_bytes()),
            })
        }
    }
}

/// Native fast path for query-parameter validation (`native=True`).
///
/// Parallel to [`try_native_fast_path`], but for the request *query string*
/// instead of the body. The query is parsed into a JSON object (string values)
/// in Rust, validated against the route's `query_schema`, and on success echoed
/// back as JSON — no GIL acquisition, no Python handler call. This lets GET
/// (and other body-less) routes serve entirely from Rust at the same ~400k RPS
/// as the body fast path.
pub(crate) fn try_native_fast_path_query(
    handler_id: usize,
    native: &[bool],
    query_schema_jsons: &[Option<String>],
    query: &[u8],
) -> Option<NativeResponse> {
    if !native.get(handler_id).copied().unwrap_or(false) {
        return None;
    }
    let schema_json = query_schema_jsons.get(handler_id).and_then(|s| s.as_ref())?;
    // Parse `a=1&b=2` into `(key, value)` pairs. Each value is coerced to a JSON
    // scalar when it looks like one (e.g. `30` -> number, `true` -> bool) so
    // typed JSON Schemas validate, matching pydantic/FastAPI query coercion.
    // Last value wins for repeated keys (repeats are rare on validated GETs).
    let pairs: Vec<(String, String)> = match serde_urlencoded::from_bytes(query) {
        Ok(p) => p,
        Err(_) => {
            return Some(validation_error_response("Query string is not valid form encoding"))
        }
    };
    let mut obj = serde_json::Map::new();
    for (k, v) in pairs {
        let val = serde_json::from_str::<serde_json::Value>(&v)
            .unwrap_or_else(|_| serde_json::Value::String(v));
        obj.insert(k, val);
    }
    let query_json = match serde_json::to_vec(&serde_json::Value::Object(obj)) {
        Ok(b) => b,
        Err(_) => return None,
    };
    match justapi_core::validate::validate_json_schema(&query_json, schema_json) {
        Ok(()) => Some(NativeResponse {
            status: 200,
            headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
            body: NativeBody::Bytes(query_json),
        }),
        Err(verr) => Some(validation_error_response(&verr.to_string())),
    }
}

/// Build a 422 response for a validation failure using the canonical
/// `{"detail": ...}` envelope.
fn validation_error_response(detail: &str) -> NativeResponse {
    let error_body = serde_json::json!({ "detail": detail }).to_string();
    NativeResponse {
        status: 422,
        headers: vec![(b"content-type".to_vec(), b"application/json".to_vec())],
        body: NativeBody::Bytes(error_body.into_bytes()),
    }
}

/// Render an exception's ``detail`` attribute as a JSON value.
///
/// List/dict ``detail`` (FastAPI-style validation errors) are serialized via
/// Python ``json.dumps`` so they round-trip as structured JSON; plain values
/// fall back to ``str()`` so they are never silently dropped.
fn exception_detail(py: Python<'_>, val: &Bound<'_, PyAny>) -> serde_json::Value {
    match val.getattr("detail") {
        Ok(d) => {
            if let Ok(json) = py.import("json") {
                if let Ok(s) = json.call_method1("dumps", (d.clone(),)) {
                    if let Ok(s) = s.extract::<String>() {
                        if let Ok(v) = serde_json::from_str::<serde_json::Value>(&s) {
                            return v;
                        }
                    }
                }
            }
            let s = d.str().ok().map(|s| s.to_string_lossy().to_string()).unwrap_or_default();
            serde_json::Value::String(s)
        }
        Err(_) => serde_json::Value::String(String::new()),
    }
}

/// Python-facing native API app — the FastAPI replacement.
///
/// Usage:
/// ```python
/// from justapi import JustAPIApp, Schema
///
/// class UserSchema(Schema):
///     name: str
///     email: str
///     age: int | None = None
///
/// app = JustAPIApp()
///
/// app.post("/users", create_user, schema=UserSchema)
/// app.run("127.0.0.1:8080")
/// ```
#[allow(clippy::too_many_arguments)]
pub(crate) fn make_native_handler<B>(
    router: Arc<Router<usize>>,
    handlers: Arc<Vec<Py<PyAny>>>,
    schemas: Arc<Vec<Option<Py<PyAny>>>>,
    schema_jsons: Arc<Vec<Option<String>>>,
    query_schema_jsons: Arc<Vec<Option<String>>>,
    batchers: Arc<Vec<Option<justapi_core::batching::Batcher<BatchedReq, NativeResponse>>>>,
    db_pool: Option<AnyPool>,
    db_url_str: Option<String>,
    scheme: String,
    app: Option<Py<PyAny>>,
    needs_request: Arc<Vec<bool>>,
    native: Arc<Vec<bool>>,
    schema_validators: Arc<Vec<Option<justapi_core::validate::CompiledValidator>>>,
    max_body_size: usize,
) -> justapi_core::middleware::HandlerFn<B>
where
    B: http_body::Body<Data = Bytes> + Send + Sync + Unpin + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let db_pool = Arc::new(db_pool);
    let db_url = Arc::new(db_url_str);
    let app = Arc::new(app);
    Arc::new(move |req: Request<B>| {
        let router = router.clone();
        let handlers = handlers.clone();
        let schemas = schemas.clone();
        let schema_jsons = schema_jsons.clone();
        let query_schema_jsons = query_schema_jsons.clone();
        let batchers = batchers.clone();
        let db_pool = db_pool.clone();
        let db_url = db_url.clone();
        let app = app.clone();
        let req_scheme = scheme.clone();
        let req_client =
            req.extensions().get::<std::net::SocketAddr>().map(|a| (a.ip().to_string(), a.port()));
        let http_version = match req.version() {
            http::Version::HTTP_10 => "1.0",
            http::Version::HTTP_11 => "1.1",
            http::Version::HTTP_2 => "2",
            http::Version::HTTP_3 => "3",
            _ => "1.1",
        }
        .to_string();
        let needs_request_clone = needs_request.clone();
        let native_clone = native.clone();
        let schema_validators_clone = schema_validators.clone();
        Box::pin(async move {
            let method = req.method().clone();
            let path = req.uri().path().to_string();
            let query_string = req.uri().query().unwrap_or("").as_bytes().to_vec();

            // RFC 10008: QUERY requests MUST fail when the Content-Type is
            // missing, because the query is defined by the request content
            // and its media type.
            if method == justapi_core::query_method()
                && !req.headers().contains_key(http::header::CONTENT_TYPE)
            {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    r#"{"detail":"QUERY requires a Content-Type header"}"#,
                ));
            }

            let mut headers = Vec::new();
            for (name, value) in req.headers() {
                headers.push((name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()));
            }

            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let is_multipart =
                content_type.as_deref().unwrap_or("").starts_with("multipart/form-data");
            let req_body = req.into_body();
            let (body_bytes, multipart_form_res) = if is_multipart {
                let ct = content_type.unwrap();
                match justapi_core::multipart::parse_multipart(req_body, &ct).await {
                    Ok(form) => (vec![], Some(Ok::<_, anyhow::Error>(form))),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("exceeds maximum size") {
                            return Ok(json_response(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                r#"{"detail":"payload too large"}"#,
                            ));
                        }
                        return Ok(json_response(
                            StatusCode::BAD_REQUEST,
                            r#"{"detail":"invalid multipart body"}"#,
                        ));
                    }
                }
            } else {
                let b = match http_body_util::Limited::new(req_body, max_body_size).collect().await
                {
                    Ok(c) => c.to_bytes(),
                    Err(e) if e.to_string().contains("length limit") => {
                        return Ok(json_response(
                            StatusCode::PAYLOAD_TOO_LARGE,
                            r#"{"detail":"payload too large"}"#,
                        ));
                    }
                    Err(e) => return Err(anyhow::anyhow!("Body error: {}", e)),
                };
                (b.to_vec(), None)
            };

            let matched = match router.at(&method, &path) {
                Ok(m) => m,
                Err(justapi_core::router::RouterError::MethodNotAllowed) => {
                    return Ok(json_response(
                        StatusCode::METHOD_NOT_ALLOWED,
                        r#"{"detail":"method not allowed"}"#,
                    ))
                }
                Err(justapi_core::router::RouterError::NotFound) => {
                    return Ok(json_response(StatusCode::NOT_FOUND, r#"{"detail":"not found"}"#))
                }
            };

            let handler_id = *matched.handler;

            let path_params: Vec<(String, String)> =
                matched.params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

            if let Some(Some(batcher)) = batchers.get(handler_id) {
                // Submit to batcher!
                let breq = BatchedReq {
                    method: method.to_string(),
                    path: path.clone(),
                    path_params,
                    query_string,
                    headers,
                    body: body_bytes.to_vec(),
                };
                let res = batcher.execute(breq).await.map_err(|e| anyhow::anyhow!("{}", e))?;
                return Ok(nr_to_response(res));
            }

            // Native fast path: validate the body in Rust and echo it back as the
            // response, with no blocking-pool thread hop, no GIL acquisition, and
            // no Python handler call. Skips the entire Python dispatch below.
            if let Some(nr) = try_native_fast_path(
                handler_id,
                &native_clone,
                &schema_validators_clone,
                &schema_jsons,
                &body_bytes,
            ) {
                return Ok(nr_to_response(nr));
            }

            // Native query fast path: validate the request query string in Rust and
            // echo the parsed params back, with no GIL/Python hop.
            if let Some(nr) = try_native_fast_path_query(
                handler_id,
                &native_clone,
                &query_schema_jsons,
                &query_string,
            ) {
                return Ok(nr_to_response(nr));
            }

            // Rust-side JSON Schema validation (fast path, no Python round-trip).
            if let Some(Some(schema_json)) = schema_jsons.get(handler_id) {
                let verr = match schema_validators_clone.get(handler_id).and_then(|v| v.as_ref()) {
                    Some(v) => v.validate(&body_bytes),
                    None => justapi_core::validate::validate_json_schema(&body_bytes, schema_json),
                };
                if let Err(verr) = verr {
                    return Ok(justapi_core::validation_response(&verr.to_string()));
                }
            }

            // Auto-transaction for write methods
            let is_write = matches!(method, Method::POST | Method::PUT | Method::DELETE);
            let tx = if is_write {
                if let Some(ref pool) = *db_pool {
                    match pool.begin().await {
                        Ok(tx) => Some(tx),
                        Err(e) => {
                            tracing::error!("Failed to begin transaction: {}", e);
                            return Ok(json_response(
                                StatusCode::INTERNAL_SERVER_ERROR,
                                r#"{"detail":"database error"}"#,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Offload Python execution (which acquires the GIL and blocks) to the blocking pool
            // to avoid starving Tokio's I/O worker threads.
            let handlers_clone = handlers.clone();
            let schemas_clone = schemas.clone();
            let db_url_str = db_url.clone();
            let trace_ctx = justapi_core::trace_context::get_current_trace_context();

            let nr = crate::gil_pool::run_python(move |py| {
                let mut form_dict_py: Option<pyo3::Py<pyo3::types::PyDict>> = None;
                if let Some(Ok(form)) = multipart_form_res {
                    let d = pyo3::types::PyDict::new(py);
                    for (k, v) in form.fields.iter() {
                        d.set_item(k, v).unwrap();
                    }
                    for f in form.files.iter() {
                        let headers_dict = pyo3::types::PyDict::new(py);
                        for (k, v) in f.headers.iter() {
                            headers_dict.set_item(k, v).unwrap();
                        }
                        let upload_file = crate::multipart::UploadFile::new(
                            f.filename.clone().unwrap_or_default(),
                            f.content_type.clone().unwrap_or_default(),
                            f.size,
                            headers_dict.into(),
                            f.temp_path.clone(),
                        );
                        let p = pyo3::Bound::new(py, upload_file).unwrap();
                        d.set_item(&f.field_name, p).unwrap();
                    }
                    form_dict_py = Some(d.into());
                }

                call_python_handler(
                    py,
                    &handlers_clone[handler_id],
                    schemas_clone[handler_id].as_ref(),
                    &path_params,
                    &query_string,
                    &headers,
                    &body_bytes,
                    db_url_str.as_deref(),
                    trace_ctx,
                    form_dict_py,
                    method.as_str(),
                    &path,
                    req_scheme.clone(),
                    req_client.clone(),
                    app.as_ref().as_ref().map(|a| a.clone_ref(py)),
                    needs_request_clone[handler_id],
                    http_version.clone(),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("GIL pool error: {}", e))?;

            // Commit or rollback transaction
            if let Some(tx) = tx {
                if nr.status >= 200 && nr.status < 300 {
                    if let Err(e) = tx.commit().await {
                        tracing::error!("Failed to commit transaction: {}", e);
                    }
                } else {
                    if let Err(e) = tx.rollback().await {
                        tracing::error!("Failed to rollback transaction: {}", e);
                    }
                }
            }

            Ok(nr_to_response(nr))
        })
    })
}

/// Build a handler for the test client.
#[allow(clippy::too_many_arguments)]
pub fn make_test_handler<B>(
    router: Arc<Router<usize>>,
    handlers: Arc<Vec<Py<PyAny>>>,
    schemas: Arc<Vec<Option<Py<PyAny>>>>,
    schema_jsons: Arc<Vec<Option<String>>>,
    query_schema_jsons: Arc<Vec<Option<String>>>,
    db_pool: Option<AnyPool>,
    db_url_str: Option<String>,
    app: Option<Py<PyAny>>,
    needs_request: Arc<Vec<bool>>,
    native: Arc<Vec<bool>>,
    schema_validators: Arc<Vec<Option<justapi_core::validate::CompiledValidator>>>,
    max_body_size: usize,
) -> justapi_core::middleware::HandlerFn<B>
where
    B: http_body::Body<Data = Bytes> + Send + Sync + Unpin + 'static,
    B::Error: std::error::Error + Send + Sync + 'static,
{
    let db_pool = Arc::new(db_pool);
    let db_url = Arc::new(db_url_str);
    let app = Arc::new(app);
    let req_scheme = "http".to_string();
    let req_client: Option<(String, u16)> = None;
    let http_version = "1.1".to_string();
    Arc::new(move |req: Request<B>| {
        let router = router.clone();
        let handlers = handlers.clone();
        let query_schema_jsons = query_schema_jsons.clone();
        let schemas = schemas.clone();
        let schema_jsons = schema_jsons.clone();
        let db_pool = db_pool.clone();
        let db_url = db_url.clone();
        let app = app.clone();
        let req_scheme = req_scheme.clone();
        let req_client = req_client.clone();
        let http_version = http_version.clone();
        let needs_request_clone = needs_request.clone();
        let native_clone = native.clone();
        let schema_validators_clone = schema_validators.clone();
        Box::pin(async move {
            let method = req.method().clone();
            let path = req.uri().path().to_string();
            let query_string = req.uri().query().unwrap_or("").as_bytes().to_vec();

            // RFC 10008: QUERY requests MUST fail when the Content-Type is
            // missing, because the query is defined by the request content
            // and its media type.
            if method == justapi_core::query_method()
                && !req.headers().contains_key(http::header::CONTENT_TYPE)
            {
                return Ok(json_response(
                    StatusCode::BAD_REQUEST,
                    r#"{"detail":"QUERY requires a Content-Type header"}"#,
                ));
            }

            let mut headers = Vec::new();
            for (name, value) in req.headers() {
                headers.push((name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()));
            }

            let content_type = req
                .headers()
                .get(http::header::CONTENT_TYPE)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());
            let is_multipart =
                content_type.as_deref().unwrap_or("").starts_with("multipart/form-data");

            let body_bytes = match http_body_util::Limited::new(req.into_body(), max_body_size)
                .collect()
                .await
            {
                Ok(c) => c.to_bytes(),
                Err(e) if e.to_string().contains("length limit") => {
                    return Ok(json_response(
                        StatusCode::PAYLOAD_TOO_LARGE,
                        r#"{"detail":"payload too large"}"#,
                    ));
                }
                Err(e) => return Err(anyhow::anyhow!("Body error: {}", e)),
            };

            let multipart_form_res: Option<
                Result<justapi_core::multipart::MultipartForm, anyhow::Error>,
            > = if is_multipart {
                let ct = content_type.unwrap();
                match justapi_core::multipart::parse_multipart(
                    http_body_util::Full::new(body_bytes.clone()),
                    &ct,
                )
                .await
                {
                    Ok(form) => Some(Ok::<_, anyhow::Error>(form)),
                    Err(e) => {
                        let msg = e.to_string();
                        if msg.contains("exceeds maximum size") {
                            return Ok(json_response(
                                StatusCode::PAYLOAD_TOO_LARGE,
                                r#"{"detail":"payload too large"}"#,
                            ));
                        }
                        return Ok(json_response(
                            StatusCode::BAD_REQUEST,
                            r#"{"detail":"invalid multipart body"}"#,
                        ));
                    }
                }
            } else {
                None
            };

            let matched = match router.at(&method, &path) {
                Ok(m) => m,
                Err(justapi_core::router::RouterError::MethodNotAllowed) => {
                    return Ok(justapi_core::json_response(
                        hyper::StatusCode::METHOD_NOT_ALLOWED,
                        r#"{"detail":"method not allowed"}"#,
                    ))
                }
                Err(justapi_core::router::RouterError::NotFound) => {
                    return Ok(justapi_core::json_response(
                        hyper::StatusCode::NOT_FOUND,
                        r#"{"detail":"not found"}"#,
                    ))
                }
            };

            let handler_id = *matched.handler;

            // Native fast path: validate the body in Rust and echo it back as the
            // response, with no blocking-pool thread hop, no GIL acquisition, and
            // no Python handler call.
            if let Some(nr) = try_native_fast_path(
                handler_id,
                &native_clone,
                &schema_validators_clone,
                &schema_jsons,
                &body_bytes,
            ) {
                return Ok(nr_to_response(nr));
            }

            // Native query fast path: validate the request query string in Rust and
            // echo the parsed params back, with no GIL/Python hop.
            if let Some(nr) = try_native_fast_path_query(
                handler_id,
                &native_clone,
                &query_schema_jsons,
                &query_string,
            ) {
                return Ok(nr_to_response(nr));
            }

            // Rust-side JSON Schema validation
            if let Some(Some(schema_json)) = schema_jsons.get(handler_id) {
                let verr = match schema_validators_clone.get(handler_id).and_then(|v| v.as_ref()) {
                    Some(v) => v.validate(&body_bytes),
                    None => justapi_core::validate::validate_json_schema(&body_bytes, schema_json),
                };
                if let Err(verr) = verr {
                    return Ok(justapi_core::validation_response(&verr.to_string()));
                }
            }

            let path_params: Vec<(String, String)> =
                matched.params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect();

            // Auto-transaction for write methods
            let is_write = matches!(method, Method::POST | Method::PUT | Method::DELETE);
            let tx = if is_write {
                if let Some(ref pool) = *db_pool {
                    match pool.begin().await {
                        Ok(tx) => Some(tx),
                        Err(e) => {
                            tracing::error!("Failed to begin transaction: {}", e);
                            return Ok(justapi_core::json_response(
                                hyper::StatusCode::INTERNAL_SERVER_ERROR,
                                r#"{"detail":"database error"}"#,
                            ));
                        }
                    }
                } else {
                    None
                }
            } else {
                None
            };

            // Offload Python execution (which acquires the GIL and blocks) to the blocking pool
            // to avoid starving Tokio's I/O worker threads.
            let handlers_clone = handlers.clone();
            let schemas_clone = schemas.clone();
            let db_url_str = db_url.clone();
            let trace_ctx = justapi_core::trace_context::get_current_trace_context();

            let method_clone = method.to_string();
            let path_clone = path.clone();
            let nr = crate::gil_pool::run_python(move |py| {
                let mut form_dict_py: Option<pyo3::Py<pyo3::types::PyDict>> = None;
                if let Some(Ok(form)) = &multipart_form_res {
                    let d = pyo3::types::PyDict::new(py);
                    for (k, v) in form.fields.iter() {
                        d.set_item(k, v).unwrap();
                    }
                    for f in form.files.iter() {
                        let headers_dict = pyo3::types::PyDict::new(py);
                        for (k, v) in f.headers.iter() {
                            headers_dict.set_item(k, v).unwrap();
                        }
                        let upload_file = crate::multipart::UploadFile::new(
                            f.filename.clone().unwrap_or_default(),
                            f.content_type.clone().unwrap_or_default(),
                            f.size,
                            headers_dict.into(),
                            f.temp_path.clone(),
                        );
                        let p = pyo3::Bound::new(py, upload_file).unwrap();
                        d.set_item(&f.field_name, p).unwrap();
                    }
                    form_dict_py = Some(d.into());
                }

                call_python_handler(
                    py,
                    &handlers_clone[handler_id],
                    schemas_clone[handler_id].as_ref(),
                    &path_params,
                    &query_string,
                    &headers,
                    &body_bytes,
                    db_url_str.as_deref(),
                    trace_ctx,
                    form_dict_py,
                    method_clone.as_str(),
                    &path_clone,
                    req_scheme.clone(),
                    req_client.clone(),
                    app.as_ref().as_ref().map(|a| a.clone_ref(py)),
                    needs_request_clone[handler_id],
                    http_version.clone(),
                )
            })
            .await
            .map_err(|e| anyhow::anyhow!("GIL pool error: {}", e))?;

            // Commit or rollback transaction
            if let Some(tx) = tx {
                if nr.status >= 200 && nr.status < 300 {
                    if let Err(e) = tx.commit().await {
                        tracing::error!("Failed to commit transaction: {}", e);
                    }
                } else {
                    if let Err(e) = tx.rollback().await {
                        tracing::error!("Failed to rollback transaction: {}", e);
                    }
                }
            }

            Ok(nr_to_response(nr))
        })
    })
}

pub(crate) fn call_plugin_hook(
    py: Python<'_>,
    plugin: &Py<PyAny>,
    hook_name: &str,
) -> PyResult<()> {
    let helper = get_helper(py);
    helper.call_plugin_hook.bind(py).call1((plugin, hook_name))?;
    Ok(())
}

/// Resolve a Python schema object to a JSON Schema string.
///
/// Accepts:
/// - A string (already a JSON Schema)
/// - A Schema subclass (has `_schema_json()` method)
/// - A Pydantic model (has `model_json_schema()` method)
pub(crate) fn resolve_schema_json(
    py: Python<'_>,
    schema: Option<Py<PyAny>>,
) -> PyResult<Option<String>> {
    let Some(schema) = schema else {
        return Ok(None);
    };

    let schema_bound = schema.bind(py);

    // Check if it's a string
    if let Ok(s) = schema_bound.extract::<String>() {
        return Ok(Some(s));
    }

    // Check if it's a dict (a raw JSON Schema object) — serialize it to a JSON
    // string so it can be compiled/validated by the Rust-side validator.
    if schema_bound.is_instance_of::<pyo3::types::PyDict>() {
        let json_mod = PyModule::import(py, "json")?;
        let s = json_mod.getattr("dumps")?.call1((schema_bound,))?.extract::<String>()?;
        return Ok(Some(s));
    }

    // Try calling _schema_json() (our Schema class)
    if let Ok(method) = schema_bound.getattr("_schema_json") {
        if let Ok(s) = method.call0()?.extract::<String>() {
            return Ok(Some(s));
        }
    }

    // Try Pydantic v2's model_json_schema()
    if let Ok(method) = schema_bound.getattr("model_json_schema") {
        if let Ok(schema_dict) = method.call0() {
            let json_mod = PyModule::import(py, "json")?;
            let s = json_mod.getattr("dumps")?.call1((schema_dict,))?.extract::<String>()?;
            return Ok(Some(s));
        }
    }

    // Try Pydantic v1's schema()
    if let Ok(method) = schema_bound.getattr("schema") {
        if let Ok(schema_dict) = method.call0() {
            let json_mod = PyModule::import(py, "json")?;
            let s = json_mod.getattr("dumps")?.call1((schema_dict,))?.extract::<String>()?;
            return Ok(Some(s));
        }
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "schema must be a JSON Schema string, Schema subclass, or Pydantic model",
    ))
}
