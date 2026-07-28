use pyo3::prelude::*;
use pyo3_async_runtimes::tokio::future_into_py;
use std::fs;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

struct Inner {
    temp_path: PathBuf,
    pos: u64,
    closed: bool,
}

/// Python-facing `UploadFile` class.
///
/// Mirrors FastAPI/Starlette's `UploadFile`: exposes `filename`, `content_type`,
/// `size`, `headers` and a standard file object via `.file`, plus async
/// `read`/`write`/`seek`/`close` that perform the (blocking) file I/O on
/// Rust's blocking thread pool so they stay awaitable without stalling the
/// async runtime. The actual bytes live in a Rust-managed temp file (written by
/// `justapi-core`'s multipart parser), keeping the hot path in Rust.
#[pyclass(name = "UploadFile")]
pub struct UploadFile {
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub content_type: String,
    #[pyo3(get)]
    pub size: u64,
    #[pyo3(get)]
    pub headers: Py<pyo3::types::PyDict>,
    inner: Arc<Mutex<Inner>>,
}

#[pymethods]
impl UploadFile {
    /// The underlying standard Python file object (blocking, not async).
    /// Provided for parity with FastAPI; prefer the async `read`/`write`.
    #[getter]
    fn file(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let path = self.inner.lock().unwrap_or_else(|e| e.into_inner()).temp_path.clone();
        let io = py.import("io")?;
        let f = io.call_method1("open", (path, "rb"))?;
        Ok(f.unbind())
    }

    /// Read up to `size` bytes from the current position. `size < 0` (default)
    /// reads the remainder of the file. Runs on the blocking pool.
    #[pyo3(signature = (size = -1))]
    fn read<'py>(&self, py: Python<'py>, size: i64) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        future_into_py(py, async move {
            let inner = inner.clone();
            let data = tokio::task::spawn_blocking(move || -> PyResult<Vec<u8>> {
                let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
                if g.closed {
                    return Err(pyo3::exceptions::PyValueError::new_err("file is closed"));
                }
                let mut file = fs::File::open(&g.temp_path)?;
                file.seek(SeekFrom::Start(g.pos))?;
                let meta = fs::metadata(&g.temp_path)?;
                let remaining = meta.len().saturating_sub(g.pos);
                let limit: usize = if size < 0 { usize::MAX } else { size as usize };
                let to_read = (remaining as usize).min(limit);
                let mut buf = vec![0u8; to_read];
                let n = file.read(&mut buf)?;
                buf.truncate(n);
                g.pos += n as u64;
                Ok(buf)
            })
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))??;
            Ok(data)
        })
    }

    /// Write `data` at the current position. Runs on the blocking pool.
    fn write<'py>(&self, py: Python<'py>, data: Vec<u8>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        future_into_py(py, async move {
            let inner = inner.clone();
            tokio::task::spawn_blocking(move || -> PyResult<()> {
                let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
                if g.closed {
                    return Err(pyo3::exceptions::PyValueError::new_err("file is closed"));
                }
                let mut file = fs::OpenOptions::new()
                    .write(true)
                    .create(true)
                    .truncate(false)
                    .open(&g.temp_path)?;
                file.seek(SeekFrom::Start(g.pos))?;
                file.write_all(&data)?;
                g.pos += data.len() as u64;
                Ok(())
            })
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))??;
            Ok(())
        })
    }

    /// Move to `offset` bytes from the start of the file. Returns the new
    /// position. Runs on the blocking pool.
    fn seek<'py>(&self, py: Python<'py>, offset: i64) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        future_into_py(py, async move {
            let inner = inner.clone();
            let new_pos = tokio::task::spawn_blocking(move || -> PyResult<u64> {
                let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
                if g.closed {
                    return Err(pyo3::exceptions::PyValueError::new_err("file is closed"));
                }
                let meta = fs::metadata(&g.temp_path)?;
                let len = meta.len();
                let pos =
                    if offset < 0 { len.saturating_sub((-offset) as u64) } else { offset as u64 };
                g.pos = pos;
                Ok(pos)
            })
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))??;
            Ok(new_pos)
        })
    }

    /// Close the file and delete the backing temp file. Runs on the blocking
    /// pool. Safe to call more than once.
    fn close<'py>(&self, py: Python<'py>) -> PyResult<Bound<'py, PyAny>> {
        let inner = self.inner.clone();
        future_into_py(py, async move {
            let inner = inner.clone();
            tokio::task::spawn_blocking(move || -> PyResult<()> {
                let mut g = inner.lock().unwrap_or_else(|e| e.into_inner());
                if g.closed {
                    return Ok(());
                }
                g.closed = true;
                let _ = fs::remove_file(&g.temp_path);
                Ok(())
            })
            .await
            .map_err(|e| pyo3::exceptions::PyRuntimeError::new_err(e.to_string()))??;
            Ok(())
        })
    }
}

impl UploadFile {
    pub fn new(
        filename: String,
        content_type: String,
        size: u64,
        headers: Py<pyo3::types::PyDict>,
        temp_path: PathBuf,
    ) -> Self {
        Self {
            filename,
            content_type,
            size,
            headers,
            inner: Arc::new(Mutex::new(Inner { temp_path, pos: 0, closed: false })),
        }
    }
}

impl Drop for UploadFile {
    fn drop(&mut self) {
        let g = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !g.closed {
            let _ = fs::remove_file(&g.temp_path);
        }
    }
}
