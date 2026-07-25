use pyo3::prelude::*;
use pyo3::types::{PyAnyMethods, PyDict, PyList};

/// Rust-native JWT encoding/decoding, backed by the `jsonwebtoken` crate.
/// No Python crypto library needed.
#[pyclass(name = "_JwtAuth")]
pub struct PyJwtAuth {
    encoding_key: jsonwebtoken::EncodingKey,
    decoding_key: jsonwebtoken::DecodingKey,
    algorithm: jsonwebtoken::Algorithm,
    validation: jsonwebtoken::Validation,
}

#[pymethods]
impl PyJwtAuth {
    #[new]
    #[pyo3(signature = (secret, algorithm = "HS256"))]
    fn new(secret: &str, algorithm: &str) -> PyResult<Self> {
        let alg = parse_algorithm(algorithm)?;
        Ok(Self {
            encoding_key: jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
            decoding_key: jsonwebtoken::DecodingKey::from_secret(secret.as_bytes()),
            algorithm: alg,
            validation: jsonwebtoken::Validation::new(alg),
        })
    }

    fn encode(&self, _py: Python<'_>, claims: &Bound<'_, PyDict>) -> PyResult<String> {
        let claims_json = claims_to_json(claims);
        jsonwebtoken::encode(
            &jsonwebtoken::Header::new(self.algorithm),
            &claims_json,
            &self.encoding_key,
        )
        .map_err(|e| pyo3::exceptions::PyValueError::new_err(e.to_string()))
    }

    #[pyo3(signature = (token, options = None))]
    fn decode(
        &self,
        py: Python<'_>,
        token: &str,
        options: Option<&Bound<'_, PyDict>>,
    ) -> PyResult<Py<PyDict>> {
        let mut validation = self.validation.clone();
        if let Some(opts) = options {
            for (k, v) in opts.iter() {
                let key: String = k.extract()?;
                let val: bool = v.extract()?;
                if key.as_str() == "verify_exp" {
                    validation.validate_exp = val;
                }
            }
        }
        let token_data =
            jsonwebtoken::decode::<serde_json::Value>(token, &self.decoding_key, &validation)
                .map_err(|e| {
                    pyo3::exceptions::PyValueError::new_err(format!("Invalid token: {}", e))
                })?;
        json_value_to_py_dict(py, &token_data.claims)
    }

    fn __repr__(&self) -> String {
        format!("_JwtAuth(algorithm={:?})", self.algorithm)
    }
}

fn parse_algorithm(s: &str) -> PyResult<jsonwebtoken::Algorithm> {
    match s.to_uppercase().as_str() {
        "HS256" => Ok(jsonwebtoken::Algorithm::HS256),
        "HS384" => Ok(jsonwebtoken::Algorithm::HS384),
        "HS512" => Ok(jsonwebtoken::Algorithm::HS512),
        "RS256" => Ok(jsonwebtoken::Algorithm::RS256),
        "RS384" => Ok(jsonwebtoken::Algorithm::RS384),
        "RS512" => Ok(jsonwebtoken::Algorithm::RS512),
        "ES256" => Ok(jsonwebtoken::Algorithm::ES256),
        "ES384" => Ok(jsonwebtoken::Algorithm::ES384),
        "ED25519" => Ok(jsonwebtoken::Algorithm::EdDSA),
        _ => Err(pyo3::exceptions::PyValueError::new_err(format!(
            "Unsupported algorithm: {}. Use HS256, HS384, HS512, RS256, RS384, RS512, ES256, ES384, or ED25519",
            s
        ))),
    }
}

fn claims_to_json(claims: &Bound<'_, PyDict>) -> serde_json::Value {
    let mut map = serde_json::Map::new();
    for (k, v) in claims.iter() {
        let key: String = k.extract().unwrap_or_default();
        map.insert(key, py_any_to_json(&v));
    }
    serde_json::Value::Object(map)
}

fn py_any_to_json(obj: &Bound<'_, PyAny>) -> serde_json::Value {
    if let Ok(s) = obj.extract::<String>() {
        return serde_json::Value::String(s);
    }
    if let Ok(i) = obj.extract::<i64>() {
        return serde_json::Value::Number(i.into());
    }
    if let Ok(f) = obj.extract::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
        return serde_json::Value::String(f.to_string());
    }
    if let Ok(b) = obj.extract::<bool>() {
        return serde_json::Value::Bool(b);
    }
    if let Ok(list) = obj.extract::<Vec<Bound<'_, PyAny>>>() {
        let arr: Vec<serde_json::Value> = list.iter().map(|x| py_any_to_json(x)).collect();
        return serde_json::Value::Array(arr);
    }
    serde_json::Value::Null
}

fn json_value_to_py_dict(py: Python<'_>, value: &serde_json::Value) -> PyResult<Py<PyDict>> {
    match value {
        serde_json::Value::Object(map) => {
            let d = PyDict::new(py);
            for (k, v) in map {
                d.set_item(k, json_value_to_py(py, v)?)?;
            }
            Ok(d.into())
        }
        _ => Err(pyo3::exceptions::PyTypeError::new_err("expected object")),
    }
}

fn json_value_to_py<'py>(py: Python<'py>, v: &serde_json::Value) -> PyResult<Bound<'py, PyAny>> {
    match v {
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
                list.append(json_value_to_py(py, item)?)?;
            }
            Ok(list.as_any().clone())
        }
        serde_json::Value::Object(o) => {
            let d = PyDict::new(py);
            for (k, val) in o {
                d.set_item(k, json_value_to_py(py, val)?)?;
            }
            Ok(d.as_any().clone())
        }
    }
}

pub fn register(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_class::<PyJwtAuth>()?;
    Ok(())
}
