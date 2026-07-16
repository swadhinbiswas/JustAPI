//! Python Database class — wraps PoolManager for JustAPIApp.

use pyo3::prelude::*;

use justapi_core::db::DatabaseConfig;

/// A database connection pool configuration for JustAPI.
///
/// Initialized lazily when the app starts via `JustAPIApp.run()`.
///
/// Usage:
/// ```python
/// from justapi import JustAPIApp, Database
///
/// app = JustAPIApp()
/// app.database = Database(
///     "sqlite://:memory:",
///     init_sql="CREATE TABLE items (id INTEGER PRIMARY KEY, name TEXT)",
///     pragmas=["journal_mode=WAL", "synchronous=NORMAL"],
/// )
/// app.run("127.0.0.1:8080")
/// ```
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
}

#[pymethods]
impl Database {
    #[new]
    #[pyo3(signature = (url, max_connections=10, init_sql=None, pragmas=None))]
    fn py_new(
        url: String,
        max_connections: u32,
        init_sql: Option<String>,
        pragmas: Option<Vec<String>>,
    ) -> Self {
        Self { url, max_connections, init_sql, pragmas }
    }

    fn __repr__(&self) -> String {
        format!("Database(url={})", self.url)
    }
}

impl Database {
    pub fn new(url: String, max_connections: u32) -> Self {
        Self { url, max_connections, init_sql: None, pragmas: None }
    }

    pub fn to_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            url: self.url.clone(),
            max_connections: self.max_connections,
            kind: None,
            init_sql: self.init_sql.clone(),
            pragmas: self.pragmas.clone(),
        }
    }
}
