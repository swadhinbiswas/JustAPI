use justapi_core::rate_limit::RateLimiter;
use pyo3::prelude::*;
use std::sync::Arc;

#[pyclass(name = "RateLimiter")]
pub struct PyRateLimiter {
    inner: Arc<RateLimiter>,
}

#[pymethods]
impl PyRateLimiter {
    #[staticmethod]
    pub fn new_redis<'py>(py: Python<'py>, redis_url: String) -> PyResult<Bound<'py, PyAny>> {
        let fut = async move {
            let limiter = RateLimiter::new_redis(&redis_url)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(PyRateLimiter { inner: Arc::new(limiter) })
        };
        pyo3_async_runtimes::tokio::future_into_py(py, fut)
    }

    pub fn check_limit<'py>(
        &self,
        py: Python<'py>,
        key: String,
        capacity: u64,
        replenish_rate: u64,
        tokens: u64,
    ) -> PyResult<Bound<'py, PyAny>> {
        let limiter = self.inner.clone();
        let fut = async move {
            let res = limiter
                .check_limit(&key, capacity, replenish_rate, tokens)
                .await
                .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))?;
            Ok(PyRateLimitResult { allowed: res.allowed, retry_after_ms: res.retry_after_ms })
        };
        pyo3_async_runtimes::tokio::future_into_py(py, fut)
    }
}

#[pyclass(name = "RateLimitResult", get_all, from_py_object)]
#[derive(Clone)]
pub struct PyRateLimitResult {
    pub allowed: bool,
    pub retry_after_ms: u64,
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyRateLimiter>()?;
    m.add_class::<PyRateLimitResult>()?;
    Ok(())
}
