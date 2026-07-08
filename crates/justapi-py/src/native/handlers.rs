use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::{Method, Request, Response, StatusCode};
use pyo3::prelude::*;

use justapi_core::router::Router;
use justapi_core::{json_response, middleware::HandlerFn, ResponseBody};

use justapi_core::db::AnyPool;

use super::types::*;
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
) -> NativeResponse {
    let helper = get_helper(py);

    // Set trace context in Python contextvars for distributed tracing
    if let Some((trace_id, span_id)) = trace_ctx {
        let _ = helper.set_trace_context.bind(py).call1((trace_id, span_id));
    }

    // Validate body if a schema is registered
    if let Some(schema) = schema {
        let validation_result = helper.validate_body.bind(py).call1((schema, body));
        match validation_result {
            Ok(errors) => {
                if let Ok(error_list) = errors.extract::<Vec<String>>() {
                    if !error_list.is_empty() {
                        let error_body = serde_json::json!({
                            "type": "https://justapi.dev/errors/validation",
                            "title": "Validation Error",
                            "status": 422,
                            "detail": error_list.join("; "),
                            "errors": error_list.iter().map(|e| {
                                serde_json::json!({"message": e})
                            }).collect::<Vec<_>>(),
                        })
                        .to_string();
                        return NativeResponse {
                            status: 422,
                            headers: vec![(
                                b"content-type".to_vec(),
                                b"application/problem+json".to_vec(),
                            )],
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

    let request = Bound::new(
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
        ),
    )
    .unwrap();

    let result = helper.call_handler.bind(py).call1((handler, request));

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

            let wrapped_result = helper.wrap_result.bind(py).call1((res,)).unwrap();

            let status: u16 = wrapped_result
                .get_item("status")
                .ok()
                .and_then(|v| v.extract::<u16>().ok())
                .unwrap_or(200);
            let resp_headers: Vec<(Vec<u8>, Vec<u8>)> = wrapped_result
                .get_item("headers")
                .ok()
                .and_then(|v| v.extract::<Vec<(Vec<u8>, Vec<u8>)>>().ok())
                .unwrap_or_default();
            let resp_body: Vec<u8> = wrapped_result
                .get_item("body")
                .ok()
                .and_then(|v| v.extract::<Vec<u8>>().ok())
                .unwrap_or_default();
            NativeResponse {
                status,
                headers: resp_headers,
                body: NativeBody::Bytes(resp_body),
            }
        }
        Err(e) => {
            tracing::error!("Native handler error: {}", e);
            NativeResponse {
                status: 500,
                headers: vec![],
                body: NativeBody::Bytes(format!("Internal Server Error: {}", e).into_bytes()),
            }
        }
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
        if let (Ok(n), Ok(v)) = (
            http::HeaderName::from_bytes(name),
            http::HeaderValue::from_bytes(value),
        ) {
            resp.headers_mut().insert(n, v);
        }
    }
    resp
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
pub(crate) fn make_native_handler(
    router: Arc<Router<usize>>,
    handlers: Arc<Vec<Py<PyAny>>>,
    schemas: Arc<Vec<Option<Py<PyAny>>>>,
    schema_jsons: Arc<Vec<Option<String>>>,
    batchers: Arc<Vec<Option<justapi_core::batching::Batcher<BatchedReq, NativeResponse>>>>,
    db_pool: Option<AnyPool>,
    db_url_str: Option<String>,
) -> HandlerFn {
    let db_pool = Arc::new(db_pool);
    let db_url = Arc::new(db_url_str);
    Arc::new(move |req: Request<hyper::body::Incoming>| {
        let router = router.clone();
        let handlers = handlers.clone();
        let schemas = schemas.clone();
        let schema_jsons = schema_jsons.clone();
        let batchers = batchers.clone();
        let db_pool = db_pool.clone();
        let db_url = db_url.clone();
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
                    r#"{"error":"QUERY requires a Content-Type header"}"#,
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
            let is_multipart = content_type
                .as_deref()
                .unwrap_or("")
                .starts_with("multipart/form-data");
            let req_body = req.into_body();
            let (body_bytes, multipart_form_res) = if is_multipart {
                let ct = content_type.unwrap();
                let form = justapi_core::multipart::parse_multipart(req_body, &ct).await;
                (vec![], Some(form))
            } else {
                let b = http_body_util::Limited::new(req_body, 50 * 1024 * 1024)
                    .collect()
                    .await
                    .map_err(|e| anyhow::anyhow!("Body too large or error: {}", e))?
                    .to_bytes();
                (b.to_vec(), None)
            };

            let matched = match router.at(&method, &path) {
                Ok(m) => m,
                Err(justapi_core::router::RouterError::MethodNotAllowed) => {
                    return Ok(json_response(
                        StatusCode::METHOD_NOT_ALLOWED,
                        r#"{"error":"method not allowed"}"#,
                    ))
                }
                Err(justapi_core::router::RouterError::NotFound) => {
                    return Ok(json_response(
                        StatusCode::NOT_FOUND,
                        r#"{"error":"not found"}"#,
                    ))
                }
            };

            let handler_id = *matched.handler;

            let path_params: Vec<(String, String)> = matched
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

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
                let res = batcher
                    .execute(breq)
                    .await
                    .map_err(|e| anyhow::anyhow!("{}", e))?;
                return Ok(nr_to_response(res));
            }

            // Rust-side JSON Schema validation (fast path, no Python round-trip).
            if let Some(Some(schema_json)) = schema_jsons.get(handler_id) {
                if let Err(verr) =
                    justapi_core::validate::validate_json_schema(&body_bytes, schema_json)
                {
                    let error_body = serde_json::json!({
                        "type": "https://justapi.dev/errors/validation",
                        "title": "Validation Error",
                        "status": 422,
                        "detail": verr.to_string(),
                        "errors": verr.errors.iter().map(|e| {
                            serde_json::json!({
                                "field": e.field,
                                "message": e.message,
                            })
                        }).collect::<Vec<_>>(),
                    })
                    .to_string();
                    let mut resp = Response::new(ResponseBody::new(
                        http_body_util::Full::new(Bytes::from(error_body))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                    ));
                    *resp.status_mut() = StatusCode::UNPROCESSABLE_ENTITY;
                    resp.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/problem+json"),
                    );
                    return Ok(resp);
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
                                r#"{"error":"database error"}"#,
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

            let nr = tokio::task::spawn_blocking(move || {
                Python::attach(|py| {
                    let mut form_dict_py: Option<pyo3::Py<pyo3::types::PyDict>> = None;
                    if let Some(Ok(form)) = multipart_form_res {
                        let d = pyo3::types::PyDict::new(py);
                        for (k, v) in form.fields.iter() {
                            d.set_item(k, v).unwrap();
                        }
                        for f in form.files.iter() {
                            let upload_file = crate::multipart::UploadFile::new(
                                f.filename.clone(),
                                f.content_type.clone(),
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
                    )
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking error: {}", e))?;

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
pub fn make_test_handler(
    router: Arc<Router<usize>>,
    handlers: Arc<Vec<Py<PyAny>>>,
    schemas: Arc<Vec<Option<Py<PyAny>>>>,
    schema_jsons: Arc<Vec<Option<String>>>,
    db_pool: Option<AnyPool>,
    db_url_str: Option<String>,
) -> justapi_core::middleware::HandlerFn {
    let db_pool = Arc::new(db_pool);
    let db_url = Arc::new(db_url_str);
    Arc::new(move |req: hyper::Request<hyper::body::Incoming>| {
        let router = router.clone();
        let handlers = handlers.clone();
        let schemas = schemas.clone();
        let schema_jsons = schema_jsons.clone();
        let db_pool = db_pool.clone();
        let db_url = db_url.clone();
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
                    r#"{"error":"QUERY requires a Content-Type header"}"#,
                ));
            }

            let mut headers = Vec::new();
            for (name, value) in req.headers() {
                headers.push((name.as_str().as_bytes().to_vec(), value.as_bytes().to_vec()));
            }

            let body_bytes = http_body_util::Limited::new(req.into_body(), 50 * 1024 * 1024)
                .collect()
                .await
                .map_err(|e| anyhow::anyhow!("Body too large or error: {}", e))?
                .to_bytes();

            let matched = match router.at(&method, &path) {
                Ok(m) => m,
                Err(justapi_core::router::RouterError::MethodNotAllowed) => {
                    return Ok(justapi_core::json_response(
                        hyper::StatusCode::METHOD_NOT_ALLOWED,
                        r#"{"error":"method not allowed"}"#,
                    ))
                }
                Err(justapi_core::router::RouterError::NotFound) => {
                    return Ok(justapi_core::json_response(
                        hyper::StatusCode::NOT_FOUND,
                        r#"{"error":"not found"}"#,
                    ))
                }
            };

            let handler_id = *matched.handler;

            // Rust-side JSON Schema validation
            if let Some(Some(schema_json)) = schema_jsons.get(handler_id) {
                if let Err(verr) =
                    justapi_core::validate::validate_json_schema(&body_bytes, schema_json)
                {
                    let error_body = serde_json::json!({
                        "type": "https://justapi.dev/errors/validation",
                        "title": "Validation Error",
                        "status": 422,
                        "detail": verr.to_string(),
                        "errors": verr.errors.iter().map(|e| {
                            serde_json::json!({
                                "field": e.field,
                                "message": e.message,
                            })
                        }).collect::<Vec<_>>(),
                    })
                    .to_string();
                    let mut resp = hyper::Response::new(justapi_core::ResponseBody::new(
                        http_body_util::Full::new(hyper::body::Bytes::from(error_body))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                    ));
                    *resp.status_mut() = hyper::StatusCode::UNPROCESSABLE_ENTITY;
                    resp.headers_mut().insert(
                        http::header::CONTENT_TYPE,
                        http::HeaderValue::from_static("application/problem+json"),
                    );
                    return Ok(resp);
                }
            }

            let path_params: Vec<(String, String)> = matched
                .params
                .iter()
                .map(|(k, v)| (k.to_string(), v.to_string()))
                .collect();

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
                                r#"{"error":"database error"}"#,
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
            let nr = tokio::task::spawn_blocking(move || {
                Python::attach(|py| {
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
                        None,
                        method_clone.as_str(),
                        &path_clone,
                    )
                })
            })
            .await
            .map_err(|e| anyhow::anyhow!("Spawn blocking error: {}", e))?;

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

pub(crate) fn call_plugin_hook(py: Python<'_>, plugin: &Py<PyAny>, hook_name: &str) -> PyResult<()> {
    let helper = get_helper(py);
    helper
        .call_plugin_hook
        .bind(py)
        .call1((plugin, hook_name))?;
    Ok(())
}

/// Resolve a Python schema object to a JSON Schema string.
///
/// Accepts:
/// - A string (already a JSON Schema)
/// - A Schema subclass (has `_schema_json()` method)
/// - A Pydantic model (has `model_json_schema()` method)
pub(crate) fn resolve_schema_json(py: Python<'_>, schema: Option<Py<PyAny>>) -> PyResult<Option<String>> {
    let Some(schema) = schema else {
        return Ok(None);
    };

    let schema_bound = schema.bind(py);

    // Check if it's a string
    if let Ok(s) = schema_bound.extract::<String>() {
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
            let s = json_mod
                .getattr("dumps")?
                .call1((schema_dict,))?
                .extract::<String>()?;
            return Ok(Some(s));
        }
    }

    // Try Pydantic v1's schema()
    if let Ok(method) = schema_bound.getattr("schema") {
        if let Ok(schema_dict) = method.call0() {
            let json_mod = PyModule::import(py, "json")?;
            let s = json_mod
                .getattr("dumps")?
                .call1((schema_dict,))?
                .extract::<String>()?;
            return Ok(Some(s));
        }
    }

    Err(pyo3::exceptions::PyTypeError::new_err(
        "schema must be a JSON Schema string, Schema subclass, or Pydantic model",
    ))
}