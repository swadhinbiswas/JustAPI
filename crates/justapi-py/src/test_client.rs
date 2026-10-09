//! Python JustAPITestClient — wraps Rust TestClient via PyO3.
//!
//! Usage:
//! ```python
//! from justapi import JustAPIApp, JustAPITestClient
//!
//! app = JustAPIApp()
//! app.get("/hello", lambda r: {"message": "hello"})
//!
//! client = JustAPITestClient(app)
//! resp = client.get("/hello")
//! assert resp.status == 200
//!
//! # With database:
//! client = JustAPITestClient(app, database="sqlite::memory:")
//! resp = client.post("/users", b'{"name":"Alice"}')
//! assert resp.status == 201
//! ```

use std::sync::Arc;

use pyo3::prelude::*;
use pyo3::types::*;

use hyper::header::HeaderName;
use justapi_core::coalesce::RequestCoalescer;
use justapi_core::db::{AnyPool, DatabaseConfig, PoolManager};
use justapi_core::middleware::{HandlerFn, MiddlewareChain};
use justapi_core::testing::TestClient;

use crate::native::JustAPIApp;

/// Python test client for JustAPI.
///
/// Sends requests through the full JustAPI pipeline
/// without starting a TCP server.
#[pyclass]
pub struct JustAPITestClient {
    test_client: Option<TestClient>,
    #[allow(dead_code)]
    db_pool: Option<AnyPool>,
    /// Cookie jar persisted across requests (name, value). Mirrors the
    /// FastAPI/httpx behaviour where a test client keeps `Set-Cookie` state.
    cookies: std::sync::Arc<std::sync::Mutex<Vec<(String, String)>>>,
}

#[pymethods]
impl JustAPITestClient {
    #[new]
    #[pyo3(signature = (app, database=None))]
    fn new(app: Py<JustAPIApp>, py: Python<'_>, database: Option<String>) -> PyResult<Self> {
        // Surface the app object on `request.app`.
        let app_py: Option<Py<PyAny>> = Some(app.clone_ref(py).into_bound(py).into_any().unbind());
        let app_ref = app.borrow(py);
        // Clone (don't take) the routing tables: the test client should not
        // consume the app, otherwise a second JustAPITestClient built from the
        // same app sees an empty router and returns 404 for every route. The
        // real `app.run` server is what legitimately takes ownership.
        let router = Arc::new(app_ref.router.clone());
        let handlers =
            Arc::new(app_ref.handlers.iter().map(|h| h.clone_ref(py)).collect::<Vec<_>>());
        let schemas = Arc::new(
            app_ref.schemas.iter().map(|s| s.as_ref().map(|v| v.clone_ref(py))).collect::<Vec<_>>(),
        );
        let schema_jsons = Arc::new(app_ref.schema_jsons.clone());
        let query_schema_jsons = Arc::new(app_ref.query_schema_jsons.clone());

        // Per-handler flag: does this route need a Python `Request` object?
        let needs_request: Vec<bool> = app_ref
            .handlers
            .iter()
            .map(|h| {
                h.bind(py)
                    .getattr("_needs_request")
                    .and_then(|v| v.extract::<bool>())
                    .unwrap_or(true)
            })
            .collect();
        let needs_request = Arc::new(needs_request);
        let native: Vec<bool> = app_ref.native.clone();
        let native = Arc::new(native);
        let native_async: Vec<bool> = app_ref
            .handlers
            .iter()
            .map(|h| {
                h.bind(py)
                    .getattr("_is_native_async")
                    .and_then(|v| v.extract::<bool>())
                    .unwrap_or(false)
            })
            .collect();
        let native_async = Arc::new(native_async);
        let sse_specs: Arc<Vec<Option<(u64, u64)>>> = Arc::new(app_ref.sse_specs.clone());
        let schema_validators: Vec<Option<justapi_core::validate::CompiledValidator>> = app_ref
            .schema_jsons
            .iter()
            .map(|s| s.as_ref().and_then(|j| justapi_core::validate::compile_schema(j).ok()))
            .collect();
        let schema_validators = Arc::new(schema_validators);

        // Initialize database pool if a database URL is provided.
        let (db_pool, db_url_str) = if let Some(ref url) = database {
            let config = DatabaseConfig {
                kind: Some(justapi_core::db::DbKind::from_url(url)),
                url: url.clone(),
                max_connections: 5,
                init_sql: None,
                pragmas: None,
                ..Default::default()
            };
            let pool = py
                .detach(move || -> Result<AnyPool, String> {
                    let rt = tokio::runtime::Runtime::new().map_err(|e| e.to_string())?;
                    rt.block_on(async {
                        let mut mgr = PoolManager::new();
                        mgr.init("test", config)
                            .await
                            .map_err(|e| format!("Database init error: {}", e))
                    })
                })
                .map_err(|e: String| pyo3::exceptions::PyRuntimeError::new_err(e))?;
            (Some(pool), database)
        } else {
            (None, None)
        };

        let handler = super::native::make_test_handler(
            router,
            handlers,
            schemas,
            schema_jsons,
            query_schema_jsons,
            db_url_str,
            app_py,
            needs_request,
            native,
            native_async,
            sse_specs,
            schema_validators,
            50 * 1024 * 1024,
        );

        // Mirror the server's native middleware chain so the test client
        // exercises the same pipeline: coalescing -> CORS -> secure headers
        // -> JWT. Configs are cloned (never taken) so the same app can still
        // run() afterwards or serve several test clients.
        let mut chain = MiddlewareChain::new(handler);

        if let Some(headers) = app_ref.coalesce_headers.as_ref() {
            let mut coalescer = RequestCoalescer::new();
            if !headers.is_empty() {
                let mut names = Vec::new();
                for h in headers {
                    if let Ok(n) = HeaderName::from_bytes(h.as_bytes()) {
                        names.push(n);
                    }
                }
                coalescer = coalescer.with_headers(&names);
            }
            chain.add(coalescer);
        }

        // CORS outermost of the native middlewares: it must answer OPTIONS
        // preflight before JWT and decorate auth-error responses.
        if let Some(cors) = app_ref.cors.as_ref() {
            chain.add(cors.clone());
        }

        if app_ref.secure_headers {
            let sh = app_ref.secure_headers_config.clone().unwrap_or_else(|| {
                justapi_core::middleware::SecurityHeaders::default().without_hsts()
            });
            chain.add(sh);
        }

        if let Some(jwt) = app_ref.jwt_auth.as_ref() {
            chain.add(jwt.clone());
        }

        let handler: HandlerFn = Arc::new(move |req| {
            let c = chain.clone();
            Box::pin(async move { c.run(req).await })
        });

        let test_client = TestClient::new(handler);
        Ok(Self { test_client: Some(test_client), db_pool, cookies: Default::default() })
    }

    fn get(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.get_with(&path, &hdr))
        })
    }

    /// GET with extra request headers (e.g. `Authorization`, custom `x-*`).
    #[pyo3(signature = (path, headers))]
    fn get_with(
        &self,
        py: Python<'_>,
        path: &str,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.get_with(&path, &hdr))
        })
    }

    fn post(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.post_with(&path, body, &hdr))
        })
    }

    #[pyo3(signature = (path, body, headers))]
    fn post_with(
        &self,
        py: Python<'_>,
        path: &str,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.post_with(&path, body, &hdr))
        })
    }

    fn put(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.put_with(&path, body, &hdr))
        })
    }

    /// PUT with extra request headers.
    #[pyo3(signature = (path, body, headers))]
    fn put_with(
        &self,
        py: Python<'_>,
        path: &str,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.put_with(&path, body, &hdr))
        })
    }

    fn patch(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.patch_with(&path, body, &hdr))
        })
    }

    /// PATCH with extra request headers.
    #[pyo3(signature = (path, body, headers))]
    fn patch_with(
        &self,
        py: Python<'_>,
        path: &str,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.patch_with(&path, body, &hdr))
        })
    }

    fn delete(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.delete_with(&path, &hdr))
        })
    }

    /// DELETE with extra request headers.
    #[pyo3(signature = (path, headers))]
    fn delete_with(
        &self,
        py: Python<'_>,
        path: &str,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.delete_with(&path, &hdr))
        })
    }

    /// OPTIONS with extra request headers (CORS preflight testing).
    #[pyo3(signature = (path, headers))]
    fn options_with(
        &self,
        py: Python<'_>,
        path: &str,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.options_with(&path, &hdr))
        })
    }

    /// Send an HTTP QUERY request (RFC 10008): safe, idempotent, body-carrying.
    fn query(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(Vec::new());
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.query_with(&path, body, &hdr))
        })
    }

    /// QUERY with extra request headers (RFC 10008 requires a Content-Type).
    #[pyo3(signature = (path, body, headers))]
    fn query_with(
        &self,
        py: Python<'_>,
        path: &str,
        body: Vec<u8>,
        headers: Vec<(String, String)>,
    ) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        let headers = self.merge_cookies(headers);
        self.do_request(py, move |client, rt| {
            let hdr = header_refs(&headers);
            rt.block_on(client.query_with(&path, body, &hdr))
        })
    }

    /// Current cookie jar as a `{name: value}` dict.
    fn cookies(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let d = pyo3::types::PyDict::new(py);
        let jar = self.cookies.lock().unwrap_or_else(|e| e.into_inner());
        for (k, v) in jar.iter() {
            d.set_item(k, v)?;
        }
        Ok(d.into_any().unbind())
    }

    /// Forget all persisted cookies.
    fn clear_cookies(&self) {
        let mut jar = self.cookies.lock().unwrap_or_else(|e| e.into_inner());
        jar.clear();
    }
}

/// Borrow `(String, String)` header pairs as `(&str, &str)` for the core client.
fn header_refs(headers: &[(String, String)]) -> Vec<(&str, &str)> {
    headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect()
}

impl JustAPITestClient {
    /// Merge the persisted cookie jar into request headers (unless the caller
    /// already supplied a `Cookie` header).
    fn merge_cookies(&self, headers: Vec<(String, String)>) -> Vec<(String, String)> {
        if headers.iter().any(|(k, _)| k.eq_ignore_ascii_case("cookie")) {
            return headers;
        }
        let jar = self.cookies.lock().unwrap_or_else(|e| e.into_inner());
        if jar.is_empty() {
            return headers;
        }
        let cookie = jar.iter().map(|(k, v)| format!("{k}={v}")).collect::<Vec<_>>().join("; ");
        let mut out = headers;
        out.push(("Cookie".to_string(), cookie));
        out
    }

    /// Record `Set-Cookie` headers from a response. `Max-Age=0` / expired
    /// `Expires` deletes the entry; everything else upserts name=value.
    fn store_response_cookies(&self, resp: &justapi_core::testing::TestResponse) {
        let mut jar = self.cookies.lock().unwrap_or_else(|e| e.into_inner());
        for (k, v) in &resp.headers {
            if !k.eq_ignore_ascii_case("set-cookie") {
                continue;
            }
            let Some((pair, _attrs)) = v.split_once(';').or(Some((v.as_str(), ""))) else {
                continue;
            };
            let Some((name, value)) = pair.split_once('=') else { continue };
            let name = name.trim().to_string();
            let value = value.trim().to_string();
            let lower = v.to_ascii_lowercase();
            if lower.contains("max-age=0") {
                jar.retain(|(n, _)| n != &name);
            } else if let Some(entry) = jar.iter_mut().find(|(n, _)| n == &name) {
                entry.1 = value;
            } else {
                jar.push((name, value));
            }
        }
    }

    fn do_request<F>(&self, py: Python<'_>, f: F) -> PyResult<Py<PyAny>>
    where
        F: FnOnce(
                TestClient,
                &tokio::runtime::Runtime,
            ) -> Result<justapi_core::testing::TestResponse, anyhow::Error>
            + Send,
    {
        let client = self.test_client.clone().ok_or_else(|| {
            pyo3::exceptions::PyRuntimeError::new_err("TestClient already consumed")
        })?;

        let resp = py
            .detach(move || {
                let rt =
                    tokio::runtime::Runtime::new().map_err(|e| format!("Runtime error: {}", e))?;
                f(client, &rt).map_err(|e| format!("Request failed: {}", e))
            })
            .map_err(|e: String| pyo3::exceptions::PyRuntimeError::new_err(e))?;

        self.store_response_cookies(&resp);
        Ok(response_to_py(&resp))
    }
}

fn response_to_py(resp: &justapi_core::testing::TestResponse) -> Py<PyAny> {
    Python::<'_>::try_attach(|py| {
        let d = PyDict::new(py);
        d.set_item("status", resp.status).ok();
        d.set_item("body", &resp.body[..]).ok();
        let hdrs = PyDict::new(py);
        for (k, v) in &resp.headers {
            hdrs.set_item(k.as_str(), v.as_str()).ok();
        }
        d.set_item("headers", hdrs).ok();
        d.into_any().unbind()
    })
    .expect("GIL already held in test client context")
}
