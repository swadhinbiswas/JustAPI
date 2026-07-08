use pyo3::prelude::*;
use std::fs;
use std::path::PathBuf;

/// Python-facing `UploadFile` class.
#[pyclass(name = "UploadFile")]
pub struct UploadFile {
    #[pyo3(get)]
    pub filename: String,
    #[pyo3(get)]
    pub content_type: String,

    // Store the path to the temp file
    temp_path: PathBuf,
}

#[pymethods]
impl UploadFile {
    pub fn read(&self) -> PyResult<Vec<u8>> {
        let bytes = fs::read(&self.temp_path)?;
        Ok(bytes)
    }

    pub fn close(&self) -> PyResult<()> {
        let _ = fs::remove_file(&self.temp_path);
        Ok(())
    }
}

impl UploadFile {
    pub fn new(filename: Option<String>, content_type: Option<String>, temp_path: PathBuf) -> Self {
        Self {
            filename: filename.unwrap_or_default(),
            content_type: content_type.unwrap_or_default(),
            temp_path,
        }
    }
}
