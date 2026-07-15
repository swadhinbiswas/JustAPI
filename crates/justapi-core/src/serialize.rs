use anyhow::Result;
use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::header::HeaderName;
use hyper::{Response, StatusCode};

use crate::ResponseBody;

// ---------------------------------------------------------------------------
// Low-level serialization helpers
// ---------------------------------------------------------------------------

/// Serialize a value to a JSON string.
///
/// Uses `simd-json` when the `simd-json` feature is enabled,
/// falling back to `serde_json` otherwise.
#[cfg(feature = "simd-json")]
pub fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(simd_json::serde::to_string(value)?)
}

#[cfg(not(feature = "simd-json"))]
pub fn to_json_string<T: serde::Serialize>(value: &T) -> Result<String> {
    Ok(serde_json::to_string(value)?)
}

/// Serialize a value to a JSON byte vector.
#[cfg(feature = "simd-json")]
pub fn to_json_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(simd_json::serde::to_vec(value)?)
}

#[cfg(not(feature = "simd-json"))]
pub fn to_json_vec<T: serde::Serialize>(value: &T) -> Result<Vec<u8>> {
    Ok(serde_json::to_vec(value)?)
}

// ---------------------------------------------------------------------------
// JsonResponse — typed response-model shaping
// ---------------------------------------------------------------------------

/// A JSON response shaped from any `Serialize` type (FastAPI's `response_model`
/// equivalent, but zero-copy Rust + serde — no Python GIL overhead).
///
/// Use the builder methods to set status code and headers, then call
/// [`into_response`](Self::into_response) to produce a `Response<ResponseBody>`.
///
/// # Example
///
/// ```ignore
/// #[derive(Serialize)]
/// struct User { id: u64, name: String }
///
/// let resp = JsonResponse::new(User { id: 1, name: "Alice".into() })
///     .with_status(StatusCode::CREATED)
///     .with_header("x-request-id", "abc-123")
///     .into_response();
/// ```
pub struct JsonResponse<T: serde::Serialize> {
    data: T,
    status: StatusCode,
    headers: Vec<(HeaderName, String)>,
}

impl<T: serde::Serialize> JsonResponse<T> {
    /// Create a new `JsonResponse` with status 200 OK and no extra headers.
    pub fn new(data: T) -> Self {
        Self { data, status: StatusCode::OK, headers: Vec::new() }
    }

    /// Set the HTTP status code.
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    /// Add a response header.
    pub fn with_header(mut self, name: &str, value: &str) -> Self {
        if let Ok(h) = HeaderName::from_bytes(name.as_bytes()) {
            self.headers.push((h, value.to_string()));
        }
        self
    }

    /// Serialize the wrapped data and produce a `Response<ResponseBody>`.
    pub fn into_response(self) -> Result<Response<ResponseBody>> {
        let body_bytes = to_json_vec(&self.data)?;
        let mut builder = Response::builder().status(self.status);
        builder = builder.header("content-type", "application/json");
        builder = builder.header("content-length", body_bytes.len().to_string());
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        let resp = builder
            .body(UnsyncBoxBody::new(
                http_body_util::Full::new(Bytes::from(body_bytes))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))
            .unwrap();
        Ok(resp)
    }
}

/// Convenience: create a `JsonResponse` with status 200 from any Serialize value.
pub fn json_response_from<T: serde::Serialize>(data: T) -> JsonResponse<T> {
    JsonResponse::new(data)
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use serde::Serialize;

    #[derive(Serialize)]
    struct TestPayload {
        message: String,
        count: u32,
    }

    #[test]
    fn test_to_json_string() {
        let payload = TestPayload { message: "hello".into(), count: 42 };
        let json = to_json_string(&payload).unwrap();
        assert!(json.contains(r#""message":"hello""#));
        assert!(json.contains(r#""count":42"#));
    }

    #[test]
    fn test_to_json_vec() {
        let payload = TestPayload { message: "hello".into(), count: 42 };
        let vec = to_json_vec(&payload).unwrap();
        let json = String::from_utf8(vec).unwrap();
        assert!(json.contains(r#""message":"hello""#));
    }

    // --- JsonResponse tests ---

    #[derive(Serialize)]
    struct UserResp {
        id: u64,
        name: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        email: Option<String>,
    }

    #[tokio::test]
    async fn json_response_default_status() {
        let user = UserResp { id: 1, name: "Alice".into(), email: None };
        let resp = JsonResponse::new(user).into_response().unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["id"], 1);
        assert_eq!(json["name"], "Alice");
        // email field skipped because None
        assert!(json.get("email").is_none());
    }

    #[tokio::test]
    async fn json_response_custom_status_and_headers() {
        let user = UserResp { id: 2, name: "Bob".into(), email: Some("bob@example.com".into()) };
        let resp = JsonResponse::new(user)
            .with_status(StatusCode::CREATED)
            .with_header("x-request-id", "req-999")
            .with_header("x-version", "2.0")
            .into_response()
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers()["x-request-id"], "req-999");
        assert_eq!(resp.headers()["x-version"], "2.0");
        assert_eq!(resp.headers()["content-type"], "application/json");
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["email"], "bob@example.com");
    }

    #[tokio::test]
    async fn json_response_from_convenience() {
        let user = UserResp { id: 3, name: "Carol".into(), email: None };
        let resp = json_response_from(user).into_response().unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn json_response_serde_rename() {
        #[derive(Serialize)]
        struct Renamed {
            #[serde(rename = "userId")]
            user_id: u64,
        }
        let resp = JsonResponse::new(Renamed { user_id: 42 }).into_response().unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["userId"], 42);
        assert!(json.get("user_id").is_none());
    }

    #[tokio::test]
    async fn json_response_flatten() {
        use std::collections::HashMap;

        #[derive(Serialize)]
        struct WithMeta {
            data: String,
            #[serde(flatten)]
            meta: HashMap<String, serde_json::Value>,
        }

        let mut meta = HashMap::new();
        meta.insert("page".into(), serde_json::json!(1));
        let val = WithMeta { data: "content".into(), meta };
        let resp = JsonResponse::new(val).into_response().unwrap();
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(json["data"], "content");
        assert_eq!(json["page"], 1);
    }
}
