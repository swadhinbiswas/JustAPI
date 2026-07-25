use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyInt, PyList, PyString, PyTuple};

use pyo3_async_runtimes::tokio::future_into_py;

/// Shared connection data backing both `Request` and `HTTPConnection`.
///
/// Mirrors the ASGI connection scope plus the buffered payload, so the
/// Starlette-style attribute surface can be served without re-parsing.
pub(crate) struct Conn {
    pub(crate) method: String,
    pub(crate) path: String,
    pub(crate) path_params_raw: Vec<(String, String)>,
    pub(crate) query_string_raw: Vec<u8>,
    pub(crate) headers_raw: Vec<(Vec<u8>, Vec<u8>)>,
    pub(crate) scheme: String,
    pub(crate) client: Option<(String, u16)>,
    pub(crate) app: Option<Py<PyAny>>,
    pub(crate) http_version: String,
    pub(crate) state: Py<PyDict>,
}

pub(crate) fn host_of(conn: &Conn) -> Option<String> {
    for (k, v) in &conn.headers_raw {
        if k.eq_ignore_ascii_case(b"host") {
            return Some(String::from_utf8_lossy(v).to_string());
        }
    }
    None
}

fn split_host_port(host: &str) -> (String, Option<u16>) {
    if let Some(idx) = host.rfind(':') {
        let (h, p) = host.split_at(idx);
        if p.trim_start_matches(':').chars().all(|c| c.is_ascii_digit()) {
            return (h.to_string(), p.trim_start_matches(':').parse::<u16>().ok());
        }
    }
    (host.to_string(), None)
}

pub(crate) fn build_headers(py: Python<'_>, conn: &Conn) -> PyResult<Py<PyAny>> {
    let raw: Vec<(Vec<u8>, Vec<u8>)> = conn.headers_raw.clone();
    Ok(Bound::new(py, Headers { raw })?.into_any().unbind())
}

pub(crate) fn build_query_params(py: Python<'_>, conn: &Conn) -> PyResult<Py<PyAny>> {
    let qs = String::from_utf8_lossy(&conn.query_string_raw).to_string();
    let mut items: Vec<(String, String)> = Vec::new();
    for pair in qs.split('&') {
        if pair.is_empty() {
            continue;
        }
        let (k, v) = match pair.split_once('=') {
            Some((k, v)) => (k, v),
            None => (pair, ""),
        };
        items.push((percent_decode(k), percent_decode(v)));
    }
    Ok(Bound::new(py, QueryParams { items })?.into_any().unbind())
}

pub(crate) fn build_cookies(py: Python<'_>, conn: &Conn) -> Py<PyAny> {
    let d = PyDict::new(py);
    for (k, v) in &conn.headers_raw {
        if k.eq_ignore_ascii_case(b"cookie") {
            let cookie = String::from_utf8_lossy(v);
            for part in cookie.split(';') {
                let part = part.trim();
                if let Some((ck, cv)) = part.split_once('=') {
                    d.set_item(ck.trim(), cv).ok();
                }
            }
        }
    }
    d.into_any().unbind()
}

pub(crate) fn build_url(
    py: Python<'_>,
    scheme: &str,
    host: &str,
    path: &str,
    query: &[u8],
    fragment: &str,
) -> PyResult<Py<PyAny>> {
    let (hostname, port) = split_host_port(host);
    let url = URL {
        scheme: scheme.to_string(),
        host: hostname,
        port,
        path: path.to_string(),
        query: String::from_utf8_lossy(query).to_string(),
        fragment: fragment.to_string(),
    };
    Ok(Bound::new(py, url)?.into_any().unbind())
}

fn build_scope(py: Python<'_>, conn: &Conn) -> Py<PyAny> {
    let d = PyDict::new(py);
    d.set_item("type", "http").ok();
    d.set_item("method", conn.method.clone()).ok();
    d.set_item("http_version", conn.http_version.clone()).ok();
    d.set_item("scheme", conn.scheme.clone()).ok();
    d.set_item("path", conn.path.clone()).ok();
    d.set_item("raw_path", py.None()).ok();
    d.set_item("query_string", PyBytes::new(py, &conn.query_string_raw)).ok();
    let hdrs = PyList::empty(py);
    for (k, v) in &conn.headers_raw {
        hdrs.append((PyBytes::new(py, k), PyBytes::new(py, v))).ok();
    }
    d.set_item("headers", hdrs).ok();
    match &conn.client {
        Some((h, p)) => d.set_item("client", (h.clone(), *p)).ok(),
        None => d.set_item("client", py.None()).ok(),
    };
    match host_of(conn) {
        Some(h) => {
            let (host, port) = split_host_port(&h);
            let server_val: (String, u16) = match port {
                Some(p) => (host.clone(), p),
                None => (host.clone(), 0u16),
            };
            d.set_item("server", server_val).ok();
        }
        None => {
            d.set_item("server", py.None()).ok();
        }
    };
    d.set_item("app", conn.app.as_ref().map(|a| a.clone_ref(py))).ok();
    d.set_item("state", conn.state.clone_ref(py)).ok();
    let pp = PyDict::new(py);
    for (k, v) in &conn.path_params_raw {
        pp.set_item(k, v).ok();
    }
    d.set_item("path_params", pp).ok();
    d.into_any().unbind()
}

fn state_get(py: Python<'_>, state: &Py<PyDict>, key: &str) -> PyResult<Option<Py<PyAny>>> {
    Ok(state.bind(py).get_item(key)?.map(|b| b.unbind()))
}

fn percent_decode(s: &str) -> String {
    let s = s.replace('+', " ");
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(hi), Some(lo)) = (hi, lo) {
                out.push((hi * 16 + lo) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).to_string()
}

// ---------------------------------------------------------------------------
// Request
// ---------------------------------------------------------------------------

#[pyclass(mapping)]
pub struct Request {
    conn: Conn,
    body_raw: Vec<u8>,
    db_url_raw: Option<String>,
    form_data: Option<Py<PyDict>>,
    path_params_cached: Option<Py<PyDict>>,
    /// Parsed/validated body (a Python object) attached when `body_schema` was
    /// validated on the fast path, so the handler receives the already-parsed
    /// value instead of re-parsing raw bytes. `None` until set or first parsed.
    parsed_body: Option<Py<PyAny>>,
}

impl Request {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        py: Python<'_>,
        method: String,
        path: String,
        path_params: Vec<(String, String)>,
        query_string: Vec<u8>,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
        body: Vec<u8>,
        db_url: Option<String>,
        form_data: Option<Py<PyDict>>,
        scheme: String,
        client: Option<(String, u16)>,
        app: Option<Py<PyAny>>,
        http_version: String,
        auth_claims: Option<String>,
    ) -> Self {
        let state = PyDict::new(py);
        // Bridge JWT claims from the Rust middleware into request.state["auth"].
        // The `auth_claims` is a JSON string of the decoded JWT payload, set by
        // the middleware and forwarded through `call_python_handler`.
        if let Some(claims_json) = &auth_claims {
            if let Ok(serde_json::Value::Object(map)) =
                serde_json::from_str::<serde_json::Value>(claims_json)
            {
                let d = PyDict::new(py);
                for (k, v) in &map {
                    if let Ok(val) = json_value_to_request(py, v) {
                        d.set_item(k.as_str(), val).ok();
                    }
                }
                state.set_item("auth", d).ok();
            }
        }
        let state = state.into();
        let conn = Conn {
            method,
            path,
            path_params_raw: path_params,
            query_string_raw: query_string,
            headers_raw: headers,
            scheme,
            client,
            app,
            http_version,
            state,
        };
        Self {
            conn,
            body_raw: body,
            db_url_raw: db_url,
            form_data,
            path_params_cached: None,
            parsed_body: None,
        }
    }

    /// Attach a parsed/validated body object (called from the dispatch layer
    /// after `body_schema` validation succeeds). Returns the cached value.
    pub fn set_parsed_body(&mut self, py: Python<'_>, value: Bound<'_, PyAny>) {
        self.parsed_body = Some(value.unbind());
        let _ = py;
    }

    /// Borrow the cached parsed body if present, else `None`.
    pub fn parsed_body(&self, py: Python<'_>) -> Option<Py<PyAny>> {
        self.parsed_body.as_ref().map(|v| v.clone_ref(py))
    }
}

fn json_value_to_request<'py>(
    py: Python<'py>,
    v: &serde_json::Value,
) -> PyResult<Bound<'py, PyAny>> {
    match v {
        serde_json::Value::Null => Ok(py.None().into_bound(py)),
        serde_json::Value::Bool(b) => Ok((*b).into_pyobject(py)?.as_any().clone()),
        serde_json::Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                Ok(i.into_pyobject(py)?.as_any().clone())
            } else if let Some(f) = n.as_f64() {
                Ok(f.into_pyobject(py)?.as_any().clone())
            } else {
                Ok(py.None().into_bound(py))
            }
        }
        serde_json::Value::String(s) => Ok(s.into_pyobject(py)?.as_any().clone()),
        serde_json::Value::Array(arr) => {
            let list = PyList::empty(py);
            for item in arr {
                list.append(json_value_to_request(py, item)?).ok();
            }
            Ok(list.as_any().clone())
        }
        serde_json::Value::Object(_) => Ok(py.None().into_bound(py)),
    }
}

#[pymethods]
impl Request {
    #[getter]
    fn method(&self) -> PyResult<String> {
        Ok(self.conn.method.clone())
    }

    #[getter]
    fn path(&self) -> PyResult<String> {
        Ok(self.conn.path.clone())
    }

    #[getter]
    fn scheme(&self) -> PyResult<String> {
        Ok(self.conn.scheme.clone())
    }

    #[getter]
    fn http_version(&self) -> PyResult<String> {
        Ok(self.conn.http_version.clone())
    }

    #[getter]
    fn client(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.conn.client.as_ref().map(|(h, p)| {
            let elems =
                [PyString::new(py, h).into_any().unbind(), PyInt::new(py, *p).into_any().unbind()];
            PyTuple::new(py, elems).expect("client tuple").into_any().unbind()
        }))
    }

    #[getter]
    fn app(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.conn.app.as_ref().map(|a| a.clone_ref(py)))
    }

    #[getter]
    fn headers(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_headers(py, &self.conn)
    }

    #[getter]
    fn query_string(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(PyBytes::new(py, &self.conn.query_string_raw).into_any().unbind())
    }

    #[getter]
    fn query_params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_query_params(py, &self.conn)
    }

    #[getter]
    fn path_params(&mut self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        if let Some(ref d) = self.path_params_cached {
            return Ok(d.bind(py).clone().into_any().unbind());
        }
        let d = PyDict::new(py);
        for (k, v) in &self.conn.path_params_raw {
            d.set_item(k.as_str(), v.as_str())?;
        }
        let ret: Py<PyDict> = d.clone().unbind();
        self.path_params_cached = Some(ret.clone_ref(py));
        Ok(d.into_any().unbind())
    }

    #[getter]
    fn cookies(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(build_cookies(py, &self.conn))
    }

    #[getter]
    fn url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, &self.conn.path, &self.conn.query_string_raw, "")
    }

    #[getter]
    fn base_url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, "", b"", "")
    }

    #[getter]
    fn scope(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(build_scope(py, &self.conn))
    }

    #[getter]
    fn state(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Bound::new(py, State { dict: self.conn.state.clone_ref(py) })?.into_any().unbind())
    }

    #[getter]
    fn user(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "user")
    }

    #[getter]
    fn auth(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "auth")
    }

    #[getter]
    fn session(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "session")
    }

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
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "request.url_for requires an app with named routes",
        ))
    }

    #[pyo3(signature = ())]
    fn body<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // Pure synchronous parse: returns the bytes directly so it works in both
        // sync and async handlers (no running event loop required). Previously this
        // wrapped the value in `future_into_py`, which raised "no running event
        // loop" for sync handlers invoked without a loop (e.g. via the test client).
        Ok(PyBytes::new(py, &self.body_raw).into_any())
    }

    #[pyo3(signature = ())]
    fn json<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        // If a `body_schema`-validated body was already parsed on the fast path,
        // return that cached object directly (no re-parse, same identity).
        if let Some(cached) = self.parsed_body(py) {
            return Ok(cached.into_bound(py));
        }
        // Pure synchronous parse: returns the parsed value directly so it works in
        // both sync and async handlers without requiring a running event loop.
        let json_module = py.import("json")?;
        let body_bytes = PyBytes::new(py, &self.body_raw);
        let loads = json_module.getattr("loads")?;
        let parsed = loads.call1((body_bytes,))?;
        Ok(parsed)
    }

    /// The body parsed and validated on the `body_schema` fast path. Returns
    /// `None` when the route registered no schema or the body was not JSON.
    #[getter]
    fn validated_body<'py>(&self, py: Python<'py>) -> PyResult<Py<PyAny>> {
        Ok(self.parsed_body(py).unwrap_or_else(|| py.None()))
    }

    #[pyo3(signature = ())]
    fn form(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        // Pure synchronous parse: returns the form dict directly so it works in
        // both sync and async handlers without requiring a running event loop.
        let result = if let Some(ref d) = self.form_data {
            d.bind(py).clone().into_any().unbind()
        } else {
            let content_type = self
                .conn
                .headers_raw
                .iter()
                .find(|(k, _)| k.eq_ignore_ascii_case(b"content-type"))
                .map(|(_, v)| String::from_utf8_lossy(v).to_lowercase())
                .unwrap_or_default();
            if content_type.starts_with("application/x-www-form-urlencoded") {
                let qs = String::from_utf8_lossy(&self.body_raw).to_string();
                let d = PyDict::new(py);
                for pair in qs.split('&') {
                    if pair.is_empty() {
                        continue;
                    }
                    let (k, v) = match pair.split_once('=') {
                        Some((k, v)) => (k, v),
                        None => (pair, ""),
                    };
                    d.set_item(percent_decode(k), percent_decode(v)).ok();
                }
                d.into_any().unbind()
            } else {
                PyDict::new(py).into_any().unbind()
            }
        };
        Ok(result)
    }

    #[pyo3(signature = ())]
    fn stream(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let s = Bound::new(
            py,
            RequestStream { body: self.body_raw.clone(), done: std::sync::Mutex::new(false) },
        )
        .unwrap()
        .into_any()
        .unbind();
        Ok(s)
    }

    #[pyo3(signature = ())]
    fn receive<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let msg = PyDict::new(py);
        msg.set_item("type", "http.request").ok();
        msg.set_item("body", PyBytes::new(py, &self.body_raw)).ok();
        msg.set_item("more_body", false).ok();
        let msg = msg.into_any().unbind();
        future_into_py(py, async move { Ok(msg) })
    }

    #[pyo3(signature = ())]
    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let none = py.None();
        future_into_py(py, async move { Ok(none) })
    }

    #[pyo3(signature = ())]
    fn is_disconnected<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        future_into_py(py, async move { Ok(false) })
    }

    #[pyo3(signature = (path))]
    fn send_push_promise<'py>(&self, py: Python<'py>, path: String) -> PyResult<Bound<'py, PyAny>> {
        let _ = path;
        future_into_py(py, async move {
            Err::<Py<PyAny>, PyErr>(pyo3::exceptions::PyRuntimeError::new_err(
                "server does not support HTTP/2 push promises",
            ))
        })
    }

    #[pyo3(signature = (key, default = None))]
    fn get(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match key {
            "method" => Ok(PyString::new(py, &self.conn.method).into_any().unbind()),
            "path" => Ok(PyString::new(py, &self.conn.path).into_any().unbind()),
            "path_params" => self.path_params(py),
            "query_string" => self.query_string(py),
            "headers" => self.headers(py),
            "query_params" => self.query_params(py),
            "cookies" => self.cookies(py),
            "body" => {
                // When a `body_schema` was validated on the fast path, return the
                // parsed object instead of raw bytes. `request["body"]` semantics
                // then match `request.json()` for schema-registered routes.
                if let Some(cached) = self.parsed_body(py) {
                    Ok(cached)
                } else {
                    Ok(PyBytes::new(py, &self.body_raw).into_any().unbind())
                }
            }
            "db_url" => match &self.db_url_raw {
                Some(db) => Ok(PyString::new(py, db).into_any().unbind()),
                None => Ok(py.None()),
            },
            "form" => match &self.form_data {
                Some(d) => Ok(d.bind(py).clone().into_any().unbind()),
                None => Ok(py.None()),
            },
            "scheme" => Ok(PyString::new(py, &self.conn.scheme).into_any().unbind()),
            "client" => self.client(py).map(|c| c.unwrap_or_else(|| py.None())),
            "app" => {
                Ok(self.conn.app.as_ref().map(|a| a.clone_ref(py)).unwrap_or_else(|| py.None()))
            }
            "url" => self.url(py),
            "base_url" => self.base_url(py),
            "scope" => self.scope(py),
            "state" => self.state(py),
            _ => {
                let state = self.conn.state.bind(py);
                if let Some(v) = state.get_item(key)? {
                    Ok(v.unbind())
                } else {
                    Ok(default.unwrap_or_else(|| py.None()))
                }
            }
        }
    }

    fn __getitem__(&mut self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let val = self.get(py, key, None)?;
        if val.is_none(py) {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
        } else {
            Ok(val)
        }
    }

    fn __setitem__(&mut self, py: Python<'_>, key: &str, value: Py<PyAny>) -> PyResult<()> {
        let state = self.conn.state.bind(py);
        state.set_item(key, value)?;
        Ok(())
    }

    fn __contains__(&mut self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let state = self.conn.state.bind(py);
        if state.contains(key)? {
            return Ok(true);
        }
        match key {
            "method" | "path" | "path_params" | "query_string" | "headers" | "query_params"
            | "cookies" | "body" | "scope" | "url" | "base_url" | "state" | "scheme" | "db_url" => {
                Ok(true)
            }
            _ => Ok(false),
        }
    }
}

// ---------------------------------------------------------------------------
// HTTPConnection (base shared by Request and WebSocket)
// ---------------------------------------------------------------------------

#[pyclass(mapping)]
pub struct HTTPConnection {
    conn: Conn,
}

impl HTTPConnection {
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        py: Python<'_>,
        method: String,
        path: String,
        path_params: Vec<(String, String)>,
        query_string: Vec<u8>,
        headers: Vec<(Vec<u8>, Vec<u8>)>,
        scheme: String,
        client: Option<(String, u16)>,
        app: Option<Py<PyAny>>,
        http_version: String,
    ) -> Self {
        let state = PyDict::new(py).into();
        let conn = Conn {
            method,
            path,
            path_params_raw: path_params,
            query_string_raw: query_string,
            headers_raw: headers,
            scheme,
            client,
            app,
            http_version,
            state,
        };
        Self { conn }
    }
}

#[pymethods]
impl HTTPConnection {
    #[getter]
    fn method(&self) -> PyResult<String> {
        Ok(self.conn.method.clone())
    }

    #[getter]
    fn path(&self) -> PyResult<String> {
        Ok(self.conn.path.clone())
    }

    #[getter]
    fn scheme(&self) -> PyResult<String> {
        Ok(self.conn.scheme.clone())
    }

    #[getter]
    fn http_version(&self) -> PyResult<String> {
        Ok(self.conn.http_version.clone())
    }

    #[getter]
    fn client(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.conn.client.as_ref().map(|(h, p)| {
            let elems =
                [PyString::new(py, h).into_any().unbind(), PyInt::new(py, *p).into_any().unbind()];
            PyTuple::new(py, elems).expect("client tuple").into_any().unbind()
        }))
    }

    #[getter]
    fn app(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        Ok(self.conn.app.as_ref().map(|a| a.clone_ref(py)))
    }

    #[getter]
    fn headers(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_headers(py, &self.conn)
    }

    #[getter]
    fn query_string(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(PyBytes::new(py, &self.conn.query_string_raw).into_any().unbind())
    }

    #[getter]
    fn query_params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        build_query_params(py, &self.conn)
    }

    #[getter]
    fn path_params(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let d = PyDict::new(py);
        for (k, v) in &self.conn.path_params_raw {
            d.set_item(k.as_str(), v.as_str())?;
        }
        Ok(d.into_any().unbind())
    }

    #[getter]
    fn cookies(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(build_cookies(py, &self.conn))
    }

    #[getter]
    fn url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, &self.conn.path, &self.conn.query_string_raw, "")
    }

    #[getter]
    fn base_url(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let host = host_of(&self.conn).unwrap_or_else(|| "localhost".to_string());
        build_url(py, &self.conn.scheme, &host, "", b"", "")
    }

    #[getter]
    fn scope(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(build_scope(py, &self.conn))
    }

    #[getter]
    fn state(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Bound::new(py, State { dict: self.conn.state.clone_ref(py) })?.into_any().unbind())
    }

    #[getter]
    fn user(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "user")
    }

    #[getter]
    fn auth(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "auth")
    }

    #[getter]
    fn session(&self, py: Python<'_>) -> PyResult<Option<Py<PyAny>>> {
        state_get(py, &self.conn.state, "session")
    }

    #[pyo3(signature = ())]
    fn receive<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let msg = PyDict::new(py);
        msg.set_item("type", "http.request").ok();
        msg.set_item("body", py.None()).ok();
        msg.set_item("more_body", false).ok();
        let msg = msg.into_any().unbind();
        future_into_py(py, async move { Ok(msg) })
    }

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
        Err(pyo3::exceptions::PyNotImplementedError::new_err(
            "request.url_for requires an app with named routes",
        ))
    }

    #[pyo3(signature = (key, default = None))]
    fn get(&self, py: Python<'_>, key: &str, default: Option<Py<PyAny>>) -> PyResult<Py<PyAny>> {
        match key {
            "method" => Ok(PyString::new(py, &self.conn.method).into_any().unbind()),
            "path" => Ok(PyString::new(py, &self.conn.path).into_any().unbind()),
            "path_params" => self.path_params(py),
            "query_string" => self.query_string(py),
            "headers" => self.headers(py),
            "query_params" => self.query_params(py),
            "cookies" => self.cookies(py),
            "scheme" => Ok(PyString::new(py, &self.conn.scheme).into_any().unbind()),
            "client" => self.client(py).map(|c| c.unwrap_or_else(|| py.None())),
            "app" => {
                Ok(self.conn.app.as_ref().map(|a| a.clone_ref(py)).unwrap_or_else(|| py.None()))
            }
            "url" => self.url(py),
            "base_url" => self.base_url(py),
            "scope" => self.scope(py),
            "state" => self.state(py),
            _ => {
                let state = self.conn.state.bind(py);
                if let Some(v) = state.get_item(key)? {
                    Ok(v.unbind())
                } else {
                    Ok(default.unwrap_or_else(|| py.None()))
                }
            }
        }
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let val = self.get(py, key, None)?;
        if val.is_none(py) {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
        } else {
            Ok(val)
        }
    }

    fn __setitem__(&mut self, py: Python<'_>, key: &str, value: Py<PyAny>) -> PyResult<()> {
        let state = self.conn.state.bind(py);
        state.set_item(key, value)?;
        Ok(())
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let state = self.conn.state.bind(py);
        if state.contains(key)? {
            return Ok(true);
        }
        match key {
            "method" | "path" | "path_params" | "query_string" | "headers" | "query_params"
            | "cookies" | "scope" | "url" | "base_url" | "state" | "scheme" => Ok(true),
            _ => Ok(false),
        }
    }
}

// ---------------------------------------------------------------------------
// Headers (Starlette-compatible immutable multidict)
// ---------------------------------------------------------------------------

#[pyclass(mapping)]
pub struct Headers {
    raw: Vec<(Vec<u8>, Vec<u8>)>,
}

#[pymethods]
impl Headers {
    /// Raw header pairs as `[(bytes, bytes), ...]`.
    #[getter]
    fn raw(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, v) in &self.raw {
            l.append((PyBytes::new(py, k), PyBytes::new(py, v)))?;
        }
        Ok(l.into_any().unbind())
    }

    #[pyo3(signature = (key, default = None))]
    fn get(&self, key: &str, default: Option<Py<PyAny>>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        for (k, v) in self.raw.iter().rev() {
            if k.eq_ignore_ascii_case(key.as_bytes()) {
                return Ok(PyString::new(py, &String::from_utf8_lossy(v)).into_any().unbind());
            }
        }
        Ok(default.unwrap_or_else(|| py.None()))
    }

    fn items(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, v) in &self.raw {
            l.append((
                PyString::new(py, &String::from_utf8_lossy(k)),
                PyString::new(py, &String::from_utf8_lossy(v)),
            ))?;
        }
        Ok(l.into_any().unbind())
    }

    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, _) in &self.raw {
            l.append(PyString::new(py, &String::from_utf8_lossy(k)))?;
        }
        Ok(l.into_any().unbind())
    }

    fn values(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (_, v) in &self.raw {
            l.append(PyString::new(py, &String::from_utf8_lossy(v)))?;
        }
        Ok(l.into_any().unbind())
    }

    fn mutable(&self, py: Python<'_>) -> bool {
        let _ = py;
        false
    }

    fn __getitem__(&self, key: &str, py: Python<'_>) -> PyResult<Py<PyAny>> {
        for (k, v) in self.raw.iter().rev() {
            if k.eq_ignore_ascii_case(key.as_bytes()) {
                return Ok(PyString::new(py, &String::from_utf8_lossy(v)).into_any().unbind());
            }
        }
        Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __contains__(&self, key: &str) -> bool {
        self.raw.iter().rev().any(|(k, _)| k.eq_ignore_ascii_case(key.as_bytes()))
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.keys(py)
    }
}

// ---------------------------------------------------------------------------
// QueryParams (Starlette-compatible multidict)
// ---------------------------------------------------------------------------

#[pyclass(mapping)]
pub struct QueryParams {
    items: Vec<(String, String)>,
}

#[pymethods]
impl QueryParams {
    #[pyo3(signature = (key, default = None))]
    fn get(&self, key: &str, default: Option<Py<PyAny>>, py: Python<'_>) -> PyResult<Py<PyAny>> {
        for (k, v) in self.items.iter().rev() {
            if k == key {
                return Ok(PyString::new(py, v).into_any().unbind());
            }
        }
        Ok(default.unwrap_or_else(|| py.None()))
    }

    fn getlist(&self, key: &str, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, v) in &self.items {
            if k == key {
                l.append(PyString::new(py, v))?;
            }
        }
        Ok(l.into_any().unbind())
    }

    fn multi_items(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, v) in &self.items {
            l.append((PyString::new(py, k), PyString::new(py, v)))?;
        }
        Ok(l.into_any().unbind())
    }

    fn items(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.multi_items(py)
    }

    fn keys(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (k, _) in &self.items {
            l.append(PyString::new(py, k))?;
        }
        Ok(l.into_any().unbind())
    }

    fn values(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let l = PyList::empty(py);
        for (_, v) in &self.items {
            l.append(PyString::new(py, v))?;
        }
        Ok(l.into_any().unbind())
    }

    fn mutable(&self, py: Python<'_>) -> bool {
        let _ = py;
        false
    }

    fn __getitem__(&self, key: &str, py: Python<'_>) -> PyResult<Py<PyAny>> {
        for (k, v) in self.items.iter().rev() {
            if k == key {
                return Ok(PyString::new(py, v).into_any().unbind());
            }
        }
        Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
    }

    fn __contains__(&self, key: &str) -> bool {
        self.items.iter().rev().any(|(k, _)| k == key)
    }

    fn __iter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        self.keys(py)
    }
}

// ---------------------------------------------------------------------------
// URL (Starlette-compatible)
// ---------------------------------------------------------------------------

#[pyclass]
#[allow(clippy::upper_case_acronyms)]
pub struct URL {
    scheme: String,
    host: String,
    port: Option<u16>,
    path: String,
    query: String,
    fragment: String,
}

#[pymethods]
impl URL {
    #[getter]
    fn scheme(&self) -> String {
        self.scheme.clone()
    }

    #[getter]
    fn hostname(&self) -> String {
        self.host.clone()
    }

    #[getter]
    fn host(&self) -> String {
        self.host.clone()
    }

    #[getter]
    fn port(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        match self.port {
            Some(p) => Ok(PyInt::new(py, p).into_any().unbind()),
            None => Ok(py.None()),
        }
    }

    #[getter]
    fn path(&self) -> String {
        self.path.clone()
    }

    #[getter]
    fn query(&self) -> String {
        self.query.clone()
    }

    #[getter]
    fn fragment(&self) -> String {
        self.fragment.clone()
    }

    fn __str__(&self) -> String {
        self.to_string_impl()
    }

    fn __repr__(&self) -> String {
        format!("URL({})", self.to_string_impl())
    }

    fn __eq__(&self, other: &URL) -> bool {
        self.to_string_impl() == other.to_string_impl()
    }
}

impl URL {
    fn to_string_impl(&self) -> String {
        let mut s = format!("{}://{}", self.scheme, self.host);
        if let Some(p) = self.port {
            let default =
                (self.scheme == "http" && p == 80) || (self.scheme == "https" && p == 443);
            if !default {
                s.push_str(&format!(":{}", p));
            }
        }
        s.push_str(&self.path);
        if !self.query.is_empty() {
            s.push('?');
            s.push_str(&self.query);
        }
        if !self.fragment.is_empty() {
            s.push('#');
            s.push_str(&self.fragment);
        }
        s
    }
}

// ---------------------------------------------------------------------------
// State (attribute + mapping access, like Starlette's state object)
// ---------------------------------------------------------------------------

#[pyclass]
pub struct State {
    dict: Py<PyDict>,
}

#[pymethods]
impl State {
    fn __getattr__(&self, py: Python<'_>, name: &str) -> PyResult<Py<PyAny>> {
        let state = self.dict.bind(py);
        if let Some(v) = state.get_item(name)? {
            Ok(v.unbind())
        } else {
            Err(pyo3::exceptions::PyAttributeError::new_err(name.to_string()))
        }
    }

    fn __setattr__(&self, py: Python<'_>, name: &str, value: Py<PyAny>) -> PyResult<()> {
        self.dict.bind(py).set_item(name, value)?;
        Ok(())
    }

    fn __delattr__(&self, py: Python<'_>, name: &str) -> PyResult<()> {
        self.dict.bind(py).del_item(name)?;
        Ok(())
    }

    fn __getitem__(&self, py: Python<'_>, key: &str) -> PyResult<Py<PyAny>> {
        let state = self.dict.bind(py);
        if let Some(v) = state.get_item(key)? {
            Ok(v.unbind())
        } else {
            Err(pyo3::exceptions::PyKeyError::new_err(key.to_string()))
        }
    }

    fn __setitem__(&self, py: Python<'_>, key: &str, value: Py<PyAny>) -> PyResult<()> {
        self.dict.bind(py).set_item(key, value)?;
        Ok(())
    }

    fn __delitem__(&self, py: Python<'_>, key: &str) -> PyResult<()> {
        self.dict.bind(py).del_item(key)?;
        Ok(())
    }

    fn __contains__(&self, py: Python<'_>, key: &str) -> PyResult<bool> {
        self.dict.bind(py).contains(key)
    }
}

// ---------------------------------------------------------------------------
// RequestStream (async iterator returned by `Request.stream()`)
// ---------------------------------------------------------------------------

#[pyclass(skip_from_py_object)]
pub struct RequestStream {
    body: Vec<u8>,
    done: std::sync::Mutex<bool>,
}

impl Clone for RequestStream {
    fn clone(&self) -> Self {
        Self { body: self.body.clone(), done: std::sync::Mutex::new(false) }
    }
}

#[pymethods]
impl RequestStream {
    fn __aiter__(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        Ok(Py::new(py, self.clone())?.into_bound(py).into_any().unbind())
    }

    fn __anext__<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let chunk = {
            let mut done = self.done.lock().unwrap();
            if *done {
                return Err(pyo3::exceptions::PyStopAsyncIteration::new_err(()));
            }
            *done = true;
            PyBytes::new(py, &self.body).into_any().unbind()
        };
        future_into_py(py, async move { Ok(chunk) })
    }
}
