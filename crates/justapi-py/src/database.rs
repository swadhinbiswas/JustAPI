use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList};

use justapi_core::db::{DatabaseConfig, IsolationLevel};

/// A database connection pool configuration for JustAPI.
///
/// Initialized lazily when the app starts via `JustAPIApp.run()`.
#[pyclass(skip_from_py_object)]
#[derive(Clone)]
pub struct Database {
    #[pyo3(get)]
    pub url: String,
    #[pyo3(get)]
    pub max_connections: u32,
    #[pyo3(get)]
    pub init_sql: Option<String>,
    /// SQLite-only PRAGMA statements applied to every pooled connection (e.g.
    /// `journal_mode=WAL`). Ignored by Postgres/MySQL.
    #[pyo3(get)]
    pub pragmas: Option<Vec<String>>,
    /// Max seconds to wait for a connection from the pool.
    #[pyo3(get)]
    pub acquire_timeout: Option<f64>,
    /// Fast-fail window (seconds) for per-request connection acquires. When the
    /// pool is saturated, a request that cannot get a connection within this
    /// window fails immediately with `503` (backpressure) instead of hanging.
    /// Defaults to 3s. (Stored as a plain `f64` because `Option<f64>` fields are
    /// dropped across the pyo3 `from_py_object` FFI for this class.)
    #[pyo3(get)]
    pub request_acquire_timeout: f64,
    /// Max seconds a connection may stay idle before recycling.
    #[pyo3(get)]
    pub idle_timeout: Option<f64>,
    /// Max seconds a connection may live before being closed.
    #[pyo3(get)]
    pub max_lifetime: Option<f64>,
    /// Seconds between background health-check pings (0/None disables).
    #[pyo3(get)]
    pub health_check_interval: Option<f64>,
    /// Default transaction isolation level: "read-uncommitted",
    /// "read-committed", "repeatable-read", "serializable", "snapshot".
    #[pyo3(get)]
    pub isolation: Option<String>,
}

#[pymethods]
impl Database {
    #[new]
    #[pyo3(signature = (url, max_connections=10, init_sql=None, pragmas=None, acquire_timeout=None, request_acquire_timeout=3.0, idle_timeout=None, max_lifetime=None, health_check_interval=None, isolation=None))]
    fn py_new(
        url: String,
        max_connections: u32,
        init_sql: Option<String>,
        pragmas: Option<Vec<String>>,
        acquire_timeout: Option<f64>,
        request_acquire_timeout: f64,
        idle_timeout: Option<f64>,
        max_lifetime: Option<f64>,
        health_check_interval: Option<f64>,
        isolation: Option<String>,
    ) -> Self {
        Self {
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
        }
    }

    fn __repr__(&self) -> String {
        format!("Database(url={})", self.url)
    }

    /// Emit the full config as a plain `dict`, built in Rust from `self` so no
    /// `from_py_object` extraction round-trip is needed (that path silently
    /// drops `Option` fields in this pyo3 version). Consumed by `set_database`.
    fn config_dict(&self, py: Python<'_>) -> Py<PyDict> {
        let d = PyDict::new(py);
        d.set_item("url", &self.url).ok();
        d.set_item("max_connections", self.max_connections).ok();
        d.set_item("init_sql", self.init_sql.clone()).ok();
        d.set_item("pragmas", self.pragmas.clone()).ok();
        d.set_item("acquire_timeout", self.acquire_timeout).ok();
        d.set_item("request_acquire_timeout", self.request_acquire_timeout).ok();
        d.set_item("idle_timeout", self.idle_timeout).ok();
        d.set_item("max_lifetime", self.max_lifetime).ok();
        d.set_item("health_check_interval", self.health_check_interval).ok();
        d.set_item("isolation", self.isolation.clone()).ok();
        d.into()
    }
}

impl Database {
    pub fn new(url: String, max_connections: u32) -> Self {
        Self {
            url,
            max_connections,
            init_sql: None,
            pragmas: None,
            acquire_timeout: Some(30.0),
            request_acquire_timeout: 3.0,
            idle_timeout: None,
            max_lifetime: Some(1800.0),
            health_check_interval: None,
            isolation: None,
        }
    }

    pub fn to_config(&self) -> DatabaseConfig {
        let isolation = self.isolation.as_deref().and_then(parse_isolation);
        DatabaseConfig {
            url: self.url.clone(),
            max_connections: self.max_connections,
            kind: None,
            init_sql: self.init_sql.clone(),
            pragmas: self.pragmas.clone(),
            acquire_timeout: self.acquire_timeout.map(std::time::Duration::from_secs_f64),
            request_acquire_timeout: Some(std::time::Duration::from_secs_f64(
                self.request_acquire_timeout,
            )),
            idle_timeout: self.idle_timeout.map(std::time::Duration::from_secs_f64),
            max_lifetime: self.max_lifetime.map(std::time::Duration::from_secs_f64),
            health_check_interval: self
                .health_check_interval
                .map(std::time::Duration::from_secs_f64),
            default_isolation: isolation,
        }
    }
}

/// Parse a Python isolation-level string into the Rust enum.
fn parse_isolation(s: &str) -> Option<IsolationLevel> {
    match s.to_ascii_lowercase().replace(['-', '_'], "").as_str() {
        "readuncommitted" => Some(IsolationLevel::ReadUncommitted),
        "readcommitted" => Some(IsolationLevel::ReadCommitted),
        "repeatableread" => Some(IsolationLevel::RepeatableRead),
        "serializable" => Some(IsolationLevel::Serializable),
        "snapshot" => Some(IsolationLevel::Snapshot),
        _ => None,
    }
}

/// A typed query parameter for the `DbPool` bridge.
///
/// Most values pass through as JSON, but some need an explicit type that JSON
/// cannot represent losslessly — chiefly `bytes` (BLOBs). Use `DbParam.bytes(...)`
/// for binary columns. The marker serializes to `{"$bytes": "<base64>"}` and is
/// rebound as a real BLOB on the Rust side (round-tripping back as
/// `{"$bytes": "<base64>"}`).
#[pyclass(name = "DbParam", from_py_object)]
#[derive(Clone)]
pub struct DbParam {
    wire: serde_json::Value,
}

#[pymethods]
impl DbParam {
    /// A BLOB/binary parameter. `data` is `bytes`.
    #[staticmethod]
    fn bytes(data: &[u8]) -> Self {
        use base64::engine::general_purpose::STANDARD as B64;
        use base64::Engine;
        let mut m = serde_json::Map::new();
        m.insert("$bytes".into(), serde_json::Value::String(B64.encode(data)));
        Self { wire: serde_json::Value::Object(m) }
    }
}

impl DbParam {
    /// The JSON wire marker the Rust side understands.
    pub fn wire(&self) -> &serde_json::Value {
        &self.wire
    }
}

/// Python-facing handle to a resolved `AnyPool`.
///
/// Every method runs the SQL in Rust (over `sqlx::Any`) with **bound
/// parameters** — no string interpolation — so it is injection-safe. The DB
/// round-trip runs with the GIL released (`py.detach`), so Python threads can
/// make progress while waiting on the database (ADR-056 follow-up / AGENTS.md §2).
#[pyclass(name = "DbPool", from_py_object)]
#[derive(Clone)]
pub struct DbPool {
    inner: justapi_core::db::AnyPool,
    rt: tokio::runtime::Handle,
}

impl DbPool {
    pub fn new(inner: justapi_core::db::AnyPool, rt: tokio::runtime::Handle) -> Self {
        Self { inner, rt }
    }

    /// Borrow the underlying core pool (used internally by the server entrypoint
    /// to wire the same pool into the Rust-native CRUD handler).
    pub fn as_any_pool(&self) -> justapi_core::db::AnyPool {
        self.inner.clone()
    }

    /// Run a DB future to completion on the pool's dedicated multi-threaded
    /// tokio runtime, returning the resolved value or a `PyRuntimeError`.
    ///
    /// Design (P2.2 / ADR-074):
    ///   * `py.detach` releases the GIL for the wait, so other Python handlers
    ///     and the server's I/O loop keep making progress (preserves the high
    ///     SELECT throughput the old `py.detach`+`block_on` path had).
    ///   * We must NOT `rt.block_on(fut)` from the calling thread: when the
    ///     caller is itself serviced by that runtime (or shares its executor)
    ///     `block_on` deadlocks — the connection pool is never returned, every
    ///     subsequent acquire blocks for `busy_timeout` and writes are silently
    ///     lost (49/50 inserts vanished in repro). Instead we `rt.spawn` the
    ///     future onto the DB runtime's own worker threads and block the caller
    ///     on an `mpsc` channel. The task runs to completion and commits; the
    ///     caller just waits, with no re-entrant runtime driving.
    ///
    /// The closure returns a plain `anyhow::Result` because `py.detach`
    /// requires its return type to be `Ungil` (no Python refs); the `PyErr`
    /// conversion happens after the GIL is re-acquired.
    fn run_blocking<'py, F, T>(&self, py: Python<'py>, fut: F) -> PyResult<T>
    where
        F: std::future::Future<Output = Result<T, anyhow::Error>> + Send + 'static,
        T: Send + 'static,
    {
        let res: Result<T, anyhow::Error> = py.detach(|| {
            let (tx, rx) = std::sync::mpsc::channel();
            self.rt.spawn(async move {
                // The future runs purely in Rust; any error is forwarded to
                // the waiting thread via the channel.
                let _ = tx.send(fut.await);
            });
            // Wait for the spawned task without driving the runtime ourselves
            // (avoids the re-entrant block_on deadlock).
            rx.recv().map_err(|_| anyhow::anyhow!("db task dropped before completion"))?
        });
        res.map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
    }
}

#[pymethods]
impl DbPool {
    /// Run a query with optional bound parameters; returns a list of row
    /// dicts. `params` is a list of JSON-serializable values (`None` for no
    /// params). e.g. `db.query("SELECT * FROM items WHERE qty > ?", [3])`.
    #[pyo3(signature = (sql, params=None))]
    fn query<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let params_json: Vec<serde_json::Value> = match params {
            Some(ps) => {
                ps.iter().map(|p| python_to_json(py, p.bind(py))).collect::<PyResult<Vec<_>>>()?
            }
            None => Vec::new(),
        };
        let inner = self.inner.clone();
        let rows: serde_json::Value = self.run_blocking(py, async move {
            inner
                .query_with_params(&sql, &params_json)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        })?;
        json_to_python(py, &rows)
    }

    /// Run a write (INSERT/UPDATE/DELETE/DDL) with optional bound parameters.
    /// Returns the number of rows affected.
    #[pyo3(signature = (sql, params=None))]
    fn execute<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let params_json: Vec<serde_json::Value> = match params {
            Some(ps) => {
                ps.iter().map(|p| python_to_json(py, p.bind(py))).collect::<PyResult<Vec<_>>>()?
            }
            None => Vec::new(),
        };
        let inner = self.inner.clone();
        let affected: u64 = self.run_blocking(py, async move {
            inner
                .execute_with_params(&sql, &params_json)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        })?;
        Ok(affected.into_pyobject(py)?.into_any())
    }

    /// NATIVE ASYNC query (ADR-093): `await`-able from async handlers.
    /// The query runs on the DB's own multi-threaded tokio runtime with the
    /// GIL RELEASED for the whole execution — zero Python stepping during the
    /// query (a native operation type, the one async win our experiments
    /// proved, ADR-090/091). The coroutine suspends on the asyncio loop and
    /// resumes when sqlx completes.
    ///
    /// Usage: `rows = await app.db.query_async("SELECT * FROM users WHERE id = ?", [id])`
    ///
    /// Unlike the blocking `query()` (which must not be called from the loop
    /// thread), `query_async` is safe on the asyncio loop thread and does not
    /// block it.
    #[pyo3(signature = (sql, params=None))]
    fn query_async<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let params_json: Vec<serde_json::Value> = match params {
            Some(ps) => {
                ps.iter().map(|p| python_to_json(py, p.bind(py))).collect::<PyResult<Vec<_>>>()?
            }
            None => Vec::new(),
        };
        let inner = self.inner.clone();
        let rt = self.rt.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let rows: serde_json::Value = rt
                .spawn(async move {
                    inner
                        .query_with_params(&sql, &params_json)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("db task join error: {e}"))
                })?
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            // Convert to Python with the GIL (re-attached by the runtime);
            // `.unbind()` yields a Send `Py<PyAny>`.
            pyo3::Python::attach(|py| json_to_python(py, &rows).map(|b| b.unbind()))
        })
    }

    /// NATIVE ASYNC write (ADR-093): `await`-able INSERT/UPDATE/DELETE/DDL.
    /// Returns the number of rows affected. Same semantics as `query_async`:
    /// runs on the DB tokio runtime, GIL released during execution.
    #[pyo3(signature = (sql, params=None))]
    fn execute_async<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: Option<Vec<Py<PyAny>>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let params_json: Vec<serde_json::Value> = match params {
            Some(ps) => {
                ps.iter().map(|p| python_to_json(py, p.bind(py))).collect::<PyResult<Vec<_>>>()?
            }
            None => Vec::new(),
        };
        let inner = self.inner.clone();
        let rt = self.rt.clone();
        pyo3_async_runtimes::tokio::future_into_py(py, async move {
            let affected: u64 = rt
                .spawn(async move {
                    inner
                        .execute_with_params(&sql, &params_json)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
                .await
                .map_err(|e| {
                    pyo3::exceptions::PyRuntimeError::new_err(format!("db task join error: {e}"))
                })?
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(affected)
        })
    }

    /// Run many `(sql, params)` statements atomically in one transaction and
    /// commit. `stmts` is a list of `[sql, params]` pairs (params optional).
    /// Returns the rows of the final statement if it was a query, else
    /// `{"rows_affected": N}`. `isolation` optionally sets the transaction
    /// isolation level (e.g. `"serializable"`); unsupported levels fall back to
    /// the engine default.
    #[pyo3(signature = (stmts, isolation=None))]
    fn transaction<'py>(
        &self,
        py: Python<'py>,
        stmts: Vec<(String, Option<Vec<Py<PyAny>>>)>,
        isolation: Option<String>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let stmts_json: Vec<(String, Vec<serde_json::Value>)> = stmts
            .into_iter()
            .map(|(sql, ps)| {
                let ps = match ps {
                    Some(ps) => ps
                        .iter()
                        .map(|p| python_to_json(py, p.bind(py)))
                        .collect::<PyResult<Vec<_>>>()?,
                    None => Vec::new(),
                };
                Ok::<_, pyo3::PyErr>((sql, ps))
            })
            .collect::<PyResult<Vec<_>>>()?;
        let iso = isolation.as_deref().and_then(parse_isolation);
        let inner = self.inner.clone();
        let res: serde_json::Value = self.run_blocking(py, async move {
            match iso {
                Some(level) => {
                    let typed: Vec<(String, Vec<_>)> = stmts_json
                        .iter()
                        .map(|(s, p)| {
                            (s.clone(), p.iter().map(justapi_core::db::Param::from).collect())
                        })
                        .collect();
                    inner
                        .transaction_with_isolation(&typed, Some(level))
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                }
                None => {
                    inner.transaction(&stmts_json).await.map_err(|e| anyhow::anyhow!(e.to_string()))
                }
            }
        })?;
        json_to_python(py, &res)
    }

    /// Stream a query in batches of `chunk` rows, returning a list of row-chunks
    /// (each chunk is itself a list of row dicts). Keeps large result sets
    /// bounded in memory on the Rust side. `params` optional.
    #[pyo3(signature = (sql, params=None, chunk=1000))]
    fn query_stream<'py>(
        &self,
        py: Python<'py>,
        sql: String,
        params: Option<Vec<Py<PyAny>>>,
        chunk: usize,
    ) -> PyResult<Bound<'py, PyAny>> {
        let params_json: Vec<serde_json::Value> = match params {
            Some(ps) => {
                ps.iter().map(|p| python_to_json(py, p.bind(py))).collect::<PyResult<Vec<_>>>()?
            }
            None => Vec::new(),
        };
        let inner = self.inner.clone();
        let chunks: Vec<serde_json::Value> = self.run_blocking(py, async move {
            let typed: Vec<_> = params_json.iter().map(justapi_core::db::Param::from).collect();
            inner
                .query_stream(&sql, &typed, chunk)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        })?;
        json_to_python(py, &serde_json::Value::Array(chunks))
    }

    /// Insert a row from a dict and return the inserted row (via `RETURNING *`).
    /// `columns` restricts which keys are written (injection guard).
    #[pyo3(signature = (table, data, columns=None))]
    fn insert<'py>(
        &self,
        py: Python<'py>,
        table: String,
        data: Py<PyAny>,
        columns: Option<Vec<String>>,
    ) -> PyResult<Bound<'py, PyAny>> {
        let data_json = python_to_json(py, data.bind(py))?;
        let cols: Vec<String> = columns.unwrap_or_else(|| {
            data_json.as_object().map(|o| o.keys().cloned().collect()).unwrap_or_default()
        });
        let inner = self.inner.clone();
        let row: serde_json::Value = self.run_blocking(py, async move {
            inner
                .insert_returning(&table, &cols, &data_json)
                .await
                .map_err(|e| anyhow::anyhow!(e.to_string()))
        })?;
        json_to_python(py, &row)
    }

    /// Ping the database; raises on failure.
    fn health<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        self.run_blocking(py, async move {
            inner.health_check().await.map_err(|e| anyhow::anyhow!(e.to_string()))
        })
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(true.into_pyobject(py)?.as_any().clone())
    }
}

/// Build a Python object from a `serde_json::Value` (the inverse of
/// `python_to_json`). Rows returned by `AnyPool` are JSON, so this is how they
/// cross back into the handler as native dicts/lists/scalars.
fn json_to_python<'py>(py: Python<'py>, val: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    match val {
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
                list.append(json_to_python(py, item)?)?;
            }
            Ok(list.as_any().clone())
        }
        serde_json::Value::Object(o) => {
            let dict = PyDict::new(py);
            for (k, v) in o {
                dict.set_item(k, json_to_python(py, v)?)?;
            }
            Ok(dict.as_any().clone())
        }
    }
}

/// Convert a Python object to a `serde_json::Value` (best-effort). Mirrors the
/// binding logic used on the Rust side so handler params round-trip cleanly.
fn python_to_json(py: Python<'_>, obj: &Bound<'_, PyAny>) -> PyResult<serde_json::Value> {
    if obj.is_none() {
        Ok(serde_json::Value::Null)
    } else if let Ok(b) = obj.extract::<bool>() {
        Ok(serde_json::Value::Bool(b))
    } else if let Ok(i) = obj.extract::<i64>() {
        Ok(serde_json::Value::from(i))
    } else if let Ok(f) = obj.extract::<f64>() {
        Ok(serde_json::Value::from(f))
    } else if let Ok(s) = obj.extract::<String>() {
        Ok(serde_json::Value::String(s))
    } else if obj.is_instance_of::<PyList>() {
        let l = obj.extract::<Py<PyList>>()?;
        let l = l.bind(py);
        let mut out = Vec::new();
        for item in l.iter() {
            out.push(python_to_json(py, &item)?);
        }
        Ok(serde_json::Value::Array(out))
    } else if obj.is_instance_of::<PyDict>() {
        let d = obj.extract::<Py<PyDict>>()?;
        let d = d.bind(py);
        let mut out = serde_json::Value::Object(serde_json::Map::new());
        for (k, v) in d.iter() {
            let key = k.extract::<String>()?;
            out[key] = python_to_json(py, &v)?;
        }
        Ok(out)
    } else if let Ok(p) = obj.extract::<DbParam>() {
        Ok(p.wire().clone())
    } else {
        Ok(serde_json::Value::String(obj.str()?.to_string()))
    }
}
