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
/// app.database = Database("postgres://user:pass@localhost/mydb")
/// app.run("127.0.0.1:8080")
/// ```
#[pyclass(from_py_object)]
#[derive(Clone)]
pub struct Database {
    #[pyo3(get)]
    pub url: String,
    #[pyo3(get)]
    pub max_connections: u32,
}

#[pymethods]
impl Database {
    #[new]
    #[pyo3(signature = (url, max_connections=10))]
    fn py_new(url: String, max_connections: u32) -> Self {
        Self {
            url,
            max_connections,
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
        }
    }

    pub fn to_config(&self) -> DatabaseConfig {
        DatabaseConfig {
            url: self.url.clone(),
            max_connections: self.max_connections,
            kind: None,
        }
    }
}
