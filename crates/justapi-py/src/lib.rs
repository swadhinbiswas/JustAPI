use pyo3::prelude::*;

pub mod buffer_test;
mod dag;
mod database;
mod multipart;
mod native;
mod rate_limit;
mod request;
mod test_client;
mod websocket;

pub use dag::{Dag, DagNode};
pub use database::Database;
pub use multipart::UploadFile;
pub use native::TokenStreamResponse;
pub use rate_limit::PyRateLimitResult;
pub use request::Request;
pub use websocket::WebSocket;

/// Start the server with hardcoded Rust routes only (no Python app).
#[pyfunction]
fn serve(addr: String) -> PyResult<()> {
    let addr: std::net::SocketAddr = addr
        .parse()
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(format!("Invalid address: {}", e)))?;
    let rt = tokio::runtime::Runtime::new()
        .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Runtime error: {}", e)))?;
    rt.block_on(async {
        let server = justapi_core::Server::new(addr);
        server
            .run()
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(format!("Server error: {}", e)))
    })
}

#[pyfunction]
fn _test_zero_copy(data: &[u8]) -> buffer_test::ZeroCopyBuffer {
    buffer_test::ZeroCopyBuffer::new(data.to_vec())
}

#[pymodule]
fn _justapi(_py: Python<'_>, m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(serve, m)?)?;
    m.add_class::<buffer_test::ZeroCopyBuffer>()?;

    rate_limit::register(m)?;
    m.add_function(wrap_pyfunction!(_test_zero_copy, m)?)?;
    m.add_class::<native::JustAPIApp>()?;
    m.add_class::<native::TokenStreamResponse>()?;
    m.add_class::<native::StreamSender>()?;
    m.add_class::<test_client::JustAPITestClient>()?;
    m.add_class::<WebSocket>()?;
    m.add_class::<Request>()?;
    m.add_class::<UploadFile>()?;
    m.add_class::<Database>()?;
    m.add_class::<Dag>()?;
    m.add_class::<DagNode>()?;
    Ok(())
}
