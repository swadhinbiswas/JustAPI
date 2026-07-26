//! Typed error types for the `justapi-core` library.
//!
//! Every public API that can fail returns a `CoreError` variant, giving
//! callers the ability to match on specific failure modes and render
//! appropriate HTTP responses. Application code (CLI, bench) can use
//! `anyhow::Result` — these errors are for library boundaries only.

use http_body_util::BodyExt;

// ---------------------------------------------------------------------------
// CoreError — the top-level typed error for justapi-core
// ---------------------------------------------------------------------------

/// All errors that can arise from the `justapi-core` library.
#[derive(Debug, thiserror::Error)]
pub enum CoreError {
    /// A request failed JSON Schema or Pydantic-style validation (HTTP 422).
    #[error("validation failed: {detail}")]
    Validation { detail: String, errors: Vec<FieldError> },

    /// No route matched the request (HTTP 404).
    #[error("route not found: {method} {path}")]
    RouteNotFound { method: String, path: String },

    /// The HTTP method is not allowed on this route (HTTP 405).
    #[error("method not allowed: {method}")]
    MethodNotAllowed { method: String },

    /// Authentication failed (HTTP 401).
    #[error("unauthorized: {reason}")]
    Auth { reason: String },

    /// Authorization failed — valid credentials but insufficient permissions (HTTP 403).
    #[error("forbidden: {reason}")]
    Forbidden { reason: String },

    /// Rate limit exceeded (HTTP 429).
    #[error("rate limited, retry after {retry_after_secs}s")]
    RateLimited { retry_after_secs: u64 },

    /// Request body exceeds the configured size limit (HTTP 413).
    #[error("request body too large: {size} bytes (max {max})")]
    PayloadTooLarge { size: usize, max: usize },

    /// An upstream dependency returned an error (HTTP 502).
    #[error("upstream error: {0}")]
    Upstream(String),

    /// A configuration error was detected at startup or runtime (HTTP 500).
    #[error("configuration error: {0}")]
    Config(String),

    /// An internal invariant was violated (HTTP 500).
    #[error("internal error: {0}")]
    Internal(String),

    /// The service is temporarily unavailable (HTTP 503).
    #[error("service unavailable: {0}")]
    ServiceUnavailable(String),

    /// The request timed out (HTTP 504).
    #[error("gateway timeout after {timeout_ms}ms")]
    GatewayTimeout { timeout_ms: u64 },

    /// An error from an anyhow-converted source (backward compatibility).
    #[error(transparent)]
    Other(#[from] anyhow::Error),
}

// ---------------------------------------------------------------------------
// CoreError → HTTP status code mapping
// ---------------------------------------------------------------------------

impl CoreError {
    /// Return the appropriate HTTP status code for this error variant.
    pub fn status_code(&self) -> hyper::StatusCode {
        use hyper::StatusCode;
        match self {
            CoreError::Validation { .. } => StatusCode::UNPROCESSABLE_ENTITY,
            CoreError::RouteNotFound { .. } => StatusCode::NOT_FOUND,
            CoreError::MethodNotAllowed { .. } => StatusCode::METHOD_NOT_ALLOWED,
            CoreError::Auth { .. } => StatusCode::UNAUTHORIZED,
            CoreError::Forbidden { .. } => StatusCode::FORBIDDEN,
            CoreError::RateLimited { .. } => StatusCode::TOO_MANY_REQUESTS,
            CoreError::PayloadTooLarge { .. } => StatusCode::PAYLOAD_TOO_LARGE,
            CoreError::Upstream(_) => StatusCode::BAD_GATEWAY,
            CoreError::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CoreError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
            CoreError::ServiceUnavailable(_) => StatusCode::SERVICE_UNAVAILABLE,
            CoreError::GatewayTimeout { .. } => StatusCode::GATEWAY_TIMEOUT,
            CoreError::Other(_) => StatusCode::INTERNAL_SERVER_ERROR,
        }
    }

    /// Render this error as an RFC 9457 Problem Details JSON body.
    ///
    /// The response includes `type`, `title`, `status`, and `detail` fields.
    /// Validation errors additionally include an `errors` array with
    /// field-level messages.
    pub fn to_problem_json(&self) -> String {
        let status: u16 = self.status_code().into();
        let title = match self {
            CoreError::Validation { .. } => "Validation Error",
            CoreError::RouteNotFound { .. } => "Not Found",
            CoreError::MethodNotAllowed { .. } => "Method Not Allowed",
            CoreError::Auth { .. } => "Unauthorized",
            CoreError::Forbidden { .. } => "Forbidden",
            CoreError::RateLimited { .. } => "Too Many Requests",
            CoreError::PayloadTooLarge { .. } => "Payload Too Large",
            CoreError::Upstream(_) => "Bad Gateway",
            CoreError::Config(_) => "Internal Server Error",
            CoreError::Internal(_) => "Internal Server Error",
            CoreError::ServiceUnavailable(_) => "Service Unavailable",
            CoreError::GatewayTimeout { .. } => "Gateway Timeout",
            CoreError::Other(_) => "Internal Server Error",
        };

        let mut map = serde_json::Map::new();
        map.insert(
            "type".into(),
            serde_json::Value::String(format!(
                "https://justapi.dev/errors/{}",
                title.to_lowercase().replace(' ', "-")
            )),
        );
        map.insert("title".into(), serde_json::Value::String(title.into()));
        map.insert("status".into(), serde_json::Value::Number(status.into()));
        map.insert("detail".into(), serde_json::Value::String(self.to_string()));

        if let CoreError::Validation { errors, .. } = self {
            let errs: Vec<serde_json::Value> = errors
                .iter()
                .map(|e| {
                    serde_json::json!({
                        "field": e.field,
                        "message": e.message,
                    })
                })
                .collect();
            map.insert("errors".into(), serde_json::Value::Array(errs));
        }

        if let CoreError::RateLimited { retry_after_secs } = self {
            map.insert("retry_after".into(), serde_json::Value::Number((*retry_after_secs).into()));
        }

        serde_json::Value::Object(map).to_string()
    }
}

// ---------------------------------------------------------------------------
// FieldError — field-level validation detail
// ---------------------------------------------------------------------------

/// A single field-level validation error.
#[derive(Debug, Clone, thiserror::Error)]
#[error("{field}: {message}")]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl FieldError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { field: field.into(), message: message.into() }
    }
}

// ---------------------------------------------------------------------------
// Conversions for backward compatibility
// ---------------------------------------------------------------------------

impl From<crate::validate::ValidationError> for CoreError {
    fn from(err: crate::validate::ValidationError) -> Self {
        let errors: Vec<FieldError> = err
            .errors
            .into_iter()
            .map(|e| FieldError { field: e.field, message: e.message })
            .collect();
        let detail = errors
            .iter()
            .map(|e| format!("{}: {}", e.field, e.message))
            .collect::<Vec<_>>()
            .join("; ");
        CoreError::Validation { detail, errors }
    }
}

// ---------------------------------------------------------------------------
// Response builder
// ---------------------------------------------------------------------------

impl CoreError {
    /// Build an HTTP response from this error.
    pub fn into_response(self) -> hyper::Response<crate::ResponseBody> {
        let status = self.status_code();
        let body = self.to_problem_json();
        let retry_after = match &self {
            CoreError::RateLimited { retry_after_secs } => Some(retry_after_secs.to_string()),
            CoreError::ServiceUnavailable(_) => Some("1".to_string()),
            _ => None,
        };

        let mut builder = hyper::Response::builder()
            .status(status)
            .header("content-type", "application/problem+json")
            .header("content-length", body.len().to_string());

        if let Some(retry) = retry_after {
            builder = builder.header("retry-after", retry);
        }

        builder
            .body(crate::UnsyncBoxBody::new(
                http_body_util::Full::new(hyper::body::Bytes::from(body))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))
            .expect("Response::builder with valid inputs should never fail")
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validation_error_to_problem_json() {
        let err = CoreError::Validation {
            detail: "name: required".into(),
            errors: vec![FieldError::new("name", "required")],
        };
        let json = err.to_problem_json();
        let v: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(v["status"], 422);
        assert_eq!(v["title"], "Validation Error");
        assert!(v["errors"].is_array());
        assert_eq!(v["errors"][0]["field"], "name");
    }

    #[test]
    fn test_route_not_found_status() {
        let err = CoreError::RouteNotFound { method: "GET".into(), path: "/missing".into() };
        assert_eq!(err.status_code(), hyper::StatusCode::NOT_FOUND);
    }

    #[test]
    fn test_rate_limited_includes_retry_after() {
        let err = CoreError::RateLimited { retry_after_secs: 5 };
        let resp = err.into_response();
        assert_eq!(resp.status(), hyper::StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(resp.headers().get("retry-after").unwrap(), "5");
    }

    #[test]
    fn test_auth_error_status() {
        let err = CoreError::Auth { reason: "missing token".into() };
        assert_eq!(err.status_code(), hyper::StatusCode::UNAUTHORIZED);
    }

    #[test]
    fn test_payload_too_large_status() {
        let err = CoreError::PayloadTooLarge { size: 100_000_000, max: 50_000_000 };
        assert_eq!(err.status_code(), hyper::StatusCode::PAYLOAD_TOO_LARGE);
    }

    #[test]
    fn test_into_response_produces_valid_json() {
        let err = CoreError::Internal("something broke".into());
        let resp = err.into_response();
        assert_eq!(resp.status(), hyper::StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(resp.headers().get("content-type").unwrap(), "application/problem+json");
    }

    #[test]
    fn test_validation_from_corevalidate() {
        let val_err = crate::validate::ValidationError::new("email", "invalid format");
        let core_err: CoreError = val_err.into();
        assert_eq!(core_err.status_code(), hyper::StatusCode::UNPROCESSABLE_ENTITY);
        if let CoreError::Validation { errors, .. } = &core_err {
            assert_eq!(errors.len(), 1);
            assert_eq!(errors[0].field, "email");
        } else {
            panic!("expected Validation variant");
        }
    }
}
