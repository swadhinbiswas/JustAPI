use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList};

use justapi_core::db::{DatabaseConfig, IsolationLevel};

/// A database connection pool configuration for JustAPI.
///
/// Initialized lazily when the app starts via `JustAPIApp.run()`.
#[pyclass(from_py_object)]
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
    #[pyo3(signature = (url, max_connections=10, init_sql=None, pragmas=None, acquire_timeout=None, idle_timeout=None, max_lifetime=None, health_check_interval=None, isolation=None))]
    fn py_new(
        url: String,
        max_connections: u32,
        init_sql: Option<String>,
        pragmas: Option<Vec<String>>,
        acquire_timeout: Option<f64>,
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
            idle_timeout,
            max_lifetime,
            health_check_interval,
            isolation,
        }
    }

    fn __repr__(&self) -> String {
        format!("Database(url={})", self.url)
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
        let rt = self.rt.clone();
        let rows: serde_json::Value = py
            .detach(move || {
                rt.block_on(async move {
                    inner
                        .query_with_params(&sql, &params_json)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
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
        let rt = self.rt.clone();
        let affected: u64 = py
            .detach(move || {
                rt.block_on(async move {
                    inner
                        .execute_with_params(&sql, &params_json)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        Ok(affected.into_pyobject(py)?.into_any())
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
        let rt = self.rt.clone();
        let res: serde_json::Value = py
            .detach(move || {
                rt.block_on(async move {
                    match iso {
                        Some(level) => {
                            let typed: Vec<(String, Vec<_>)> = stmts_json
                                .iter()
                                .map(|(s, p)| {
                                    (
                                        s.clone(),
                                        p.iter().map(justapi_core::db::Param::from).collect(),
                                    )
                                })
                                .collect();
                            inner
                                .transaction_with_isolation(&typed, Some(level))
                                .await
                                .map_err(|e| anyhow::anyhow!(e.to_string()))
                        }
                        None => inner
                            .transaction(&stmts_json)
                            .await
                            .map_err(|e| anyhow::anyhow!(e.to_string())),
                    }
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
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
        let rt = self.rt.clone();
        let chunks: Vec<serde_json::Value> = py
            .detach(move || {
                rt.block_on(async move {
                    let typed: Vec<_> =
                        params_json.iter().map(justapi_core::db::Param::from).collect();
                    inner
                        .query_stream(&sql, &typed, chunk)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
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
        let rt = self.rt.clone();
        let row: serde_json::Value = py
            .detach(move || {
                rt.block_on(async move {
                    inner
                        .insert_returning(&table, &cols, &data_json)
                        .await
                        .map_err(|e| anyhow::anyhow!(e.to_string()))
                })
            })
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
        json_to_python(py, &row)
    }

    /// Ping the database; raises on failure.
    fn health<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        let rt = self.rt.clone();
        py.detach(move || rt.block_on(async move { inner.health_check().await }))
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
