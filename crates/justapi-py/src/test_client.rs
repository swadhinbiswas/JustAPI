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
}

#[pymethods]
impl JustAPITestClient {
    #[new]
    #[pyo3(signature = (app, database=None))]
    fn new(app: Py<JustAPIApp>, py: Python<'_>, database: Option<String>) -> PyResult<Self> {
        // Surface the app object on `request.app`.
        let app_py: Option<Py<PyAny>> = Some(app.clone_ref(py).into_bound(py).into_any().unbind());
        let mut app_ref = app.borrow_mut(py);
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
            db_pool.clone(),
            db_url_str,
            app_py,
            needs_request,
            native,
            schema_validators,
            50 * 1024 * 1024,
        );

        // Apply request coalescing (singleflight) if the app enabled it, so the
        // test client exercises the same pipeline as the real server.
        let coalesce_headers = app_ref.coalesce_headers.take();
        let handler: HandlerFn = match coalesce_headers {
            Some(headers) => {
                let mut coalescer = RequestCoalescer::new();
                if !headers.is_empty() {
                    let mut names = Vec::new();
                    for h in &headers {
                        if let Ok(n) = HeaderName::from_bytes(h.as_bytes()) {
                            names.push(n);
                        }
                    }
                    coalescer = coalescer.with_headers(&names);
                }
                let mut chain = MiddlewareChain::new(handler);
                chain.add(coalescer);
                Arc::new(move |req| {
                    let c = chain.clone();
                    Box::pin(async move { c.run(req).await })
                })
            }
            None => handler,
        };

        let test_client = TestClient::new(handler);
        Ok(Self { test_client: Some(test_client), db_pool })
    }

    fn get(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        self.do_request(py, move |client, rt| rt.block_on(client.get(&path)))
    }

    fn post(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        self.do_request(py, move |client, rt| rt.block_on(client.post(&path, body)))
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
        let hdr: Vec<(&str, &str)> =
            headers.iter().map(|(k, v)| (k.as_str(), v.as_str())).collect();
        self.do_request(py, move |client, rt| rt.block_on(client.post_with(&path, body, &hdr)))
    }

    fn put(&self, py: Python<'_>, path: &str, body: Vec<u8>) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        self.do_request(py, move |client, rt| rt.block_on(client.put(&path, body)))
    }

    fn delete(&self, py: Python<'_>, path: &str) -> PyResult<Py<PyAny>> {
        let path = path.to_owned();
        self.do_request(py, move |client, rt| rt.block_on(client.delete(&path)))
    }
}

impl JustAPITestClient {
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
