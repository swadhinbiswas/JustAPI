use pyo3::prelude::*;
use pyo3::types::{PyAny, PyBytes, PyDict, PyList};

#[pyclass(mapping)]
pub struct Request {
    method: String,
    path: String,
    path_params_raw: Vec<(String, String)>,
    query_string_raw: Vec<u8>,
    headers_raw: Vec<(Vec<u8>, Vec<u8>)>,
    body_raw: Vec<u8>,
    db_url_raw: Option<String>,

    state: Py<PyDict>,
    path_params_cached: Option<Py<PyDict>>,
    form_data: Option<Py<PyDict>>,
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
    ) -> Self {
        Self {
            method,
            path,
            path_params_raw: path_params,
            query_string_raw: query_string,
            headers_raw: headers,
            body_raw: body,
            db_url_raw: db_url,
            state: PyDict::new(py).into(),
            path_params_cached: None,
            form_data,
        }
    }
}

#[pymethods]
impl Request {
    #[getter]
    fn method(&self) -> PyResult<String> {
        Ok(self.method.clone())
    }

    #[getter]
    fn path(&self) -> PyResult<String> {
        Ok(self.path.clone())
    }

    #[getter]
    fn path_params(&mut self, py: Python<'_>) -> PyResult<Py<PyDict>> {
        if let Some(ref d) = self.path_params_cached {
            return Ok(d.clone_ref(py));
        }
        let d = PyDict::new(py);
        for (k, v) in &self.path_params_raw {
            d.set_item(k.as_str(), v.as_str())?;
        }
        let ret: Py<PyDict> = d.into();
        self.path_params_cached = Some(ret.clone_ref(py));
        Ok(ret)
    }

    #[getter]
    fn headers(&self, py: Python<'_>) -> PyResult<Py<PyList>> {
        let l = PyList::empty(py);
        for (k, v) in &self.headers_raw {
            l.append((PyBytes::new(py, k), PyBytes::new(py, v)))?;
        }
        Ok(l.into())
    }

    #[getter]
    fn query_string(&self, py: Python<'_>) -> PyResult<Py<PyBytes>> {
        Ok(PyBytes::new(py, &self.query_string_raw).into())
    }

    #[getter]
    fn body(&self, py: Python<'_>) -> PyResult<Py<PyBytes>> {
        Ok(PyBytes::new(py, &self.body_raw).into())
    }

    fn json(&self, py: Python<'_>) -> PyResult<Py<PyAny>> {
        let json_module = py.import("json")?;
        let body_bytes = PyBytes::new(py, &self.body_raw);
        let parsed = json_module.getattr("loads")?.call1((body_bytes,))?;
        Ok(parsed.into())
    }

    #[pyo3(signature = (key, default=None))]
    fn get(
        &mut self,
        py: Python<'_>,
        key: &str,
        default: Option<Py<PyAny>>,
    ) -> PyResult<Py<PyAny>> {
        match key {
            "method" => Ok(pyo3::types::PyString::new(py, &self.method)
                .into_any()
                .into()),
            "path" => Ok(pyo3::types::PyString::new(py, &self.path).into_any().into()),
            "path_params" => Ok(self.path_params(py)?.into_any()),
            "query_string" => Ok(self.query_string(py)?.into_any()),
            "headers" => Ok(self.headers(py)?.into_any()),
            "body" => Ok(self.body(py)?.into_any()),
            "db_url" => {
                if let Some(db) = &self.db_url_raw {
                    let py_str = pyo3::types::PyString::new(py, db);
                    Ok(py_str.into_any().into())
                } else {
                    Ok(py.None())
                }
            }
            "form" => {
                if let Some(ref d) = self.form_data {
                    Ok(d.clone_ref(py).into_any())
                } else {
                    Ok(py.None())
                }
            }
            _ => {
                let state = self.state.bind(py);
                if let Some(v) = state.get_item(key)? {
                    Ok(v.into())
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
        let state = self.state.bind(py);
        state.set_item(key, value)?;
        Ok(())
    }

    fn __contains__(&mut self, py: Python<'_>, key: &str) -> PyResult<bool> {
        let state = self.state.bind(py);
        if state.contains(key)? {
            return Ok(true);
        }
        match key {
            "path_params" | "query_string" | "headers" | "body" | "db_url" => Ok(true),
            _ => Ok(false),
        }
    }
}
