//! PyO3 glue for justapi-core's logging/tracing subsystem.
//!
//! These are thin re-exports of the Rust `tracing`-based logging setup. The
//! framework automatically initializes a default (INFO, text→stdout) logger
//! inside `JustAPIApp.run`, so most users never call these directly. They exist
//! for apps that want structured JSON logs, file rotation, or OTLP export.

use pyo3::prelude::*;

use justapi_core::tracing_setup::{
    init_file_logging as core_init_file, init_json_logging as core_init_json,
    init_logging as core_init_logging, init_otlp_tracing as core_init_otlp,
    shutdown_tracing as core_shutdown, LogFormat, LoggingConfig,
};

/// Configure logging. If a subscriber is already installed this is a no-op.
///
/// level: "debug" | "info" | "warn" | "error" (or RUST_LOG-style filter).
/// format: "text" (default) | "json".
#[pyfunction]
#[pyo3(signature = (level="info", format="text"))]
fn init_logging(level: &str, format: &str) -> PyResult<()> {
    let fmt = match format.to_ascii_lowercase().as_str() {
        "json" => LogFormat::Json,
        _ => LogFormat::Text,
    };
    core_init_logging(&LoggingConfig {
        format: fmt,
        level: level.to_string(),
        ..Default::default()
    })
    .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Initialize JSON-formatted logging to stdout.
#[pyfunction]
fn init_json_logging() -> PyResult<()> {
    core_init_json().map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Initialize JSON logging to a rolling file path.
#[pyfunction]
fn init_file_logging(path: &str) -> PyResult<()> {
    core_init_file(path).map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Initialize tracing with an OTLP gRPC endpoint (collector export).
#[pyfunction]
#[pyo3(signature = (endpoint="http://localhost:4317", service_name="justapi"))]
fn init_otlp_tracing(endpoint: &str, service_name: &str) -> PyResult<()> {
    core_init_otlp(endpoint, service_name)
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))
}

/// Flush and shut down the tracing subscriber (e.g. before process exit).
#[pyfunction]
fn shutdown_tracing() {
    core_shutdown();
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(init_logging, m)?)?;
    m.add_function(wrap_pyfunction!(init_json_logging, m)?)?;
    m.add_function(wrap_pyfunction!(init_file_logging, m)?)?;
    m.add_function(wrap_pyfunction!(init_otlp_tracing, m)?)?;
    m.add_function(wrap_pyfunction!(shutdown_tracing, m)?)?;
    Ok(())
}
