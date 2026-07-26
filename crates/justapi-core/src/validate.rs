use std::fmt;
use std::sync::OnceLock;

use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};
use jsonschema::Validator;
use serde::de::DeserializeOwned;

use crate::ResponseBody;

// ---------------------------------------------------------------------------
// Built-in `format` validators
// ---------------------------------------------------------------------------

/// Register the built-in `format` validators (email, uuid, uri, date-time, ...)
/// once. `jsonschema` 0.46 ships `format` as an opt-in keyword; with
/// `default-features = false` no formats are asserted unless we register them,
/// so we provide lightweight Rust-side checkers. They are intentionally
/// permissive (reject obvious junk, not RFC-exhaustive) — enough for request
/// validation without pulling a format-parsing dependency.
type FormatChecker = (&'static str, fn(&str) -> bool);

fn format_validators() -> &'static [FormatChecker] {
    static FMTS: OnceLock<Vec<FormatChecker>> = OnceLock::new();
    FMTS.get_or_init(|| {
        vec![
            ("email", |s: &str| {
                if s.is_empty() || s.contains(char::is_whitespace) {
                    return false;
                }
                let (local, domain) = match s.split_once('@') {
                    Some(pair) => pair,
                    None => return false,
                };
                !local.is_empty()
                    && !domain.is_empty()
                    && domain.contains('.')
                    && !domain.starts_with('.')
                    && !domain.ends_with('.')
            }),
            ("uri", |s: &str| match s.split_once("://") {
                Some((scheme, rest)) => {
                    !scheme.is_empty()
                        && scheme
                            .chars()
                            .all(|c| c.is_alphanumeric() || c == '+' || c == '-' || c == '.')
                        && !rest.is_empty()
                }
                None => false,
            }),
            ("uuid", |s: &str| {
                let s = s.trim_matches(|c| c == '{' || c == '}');
                let parts: Vec<&str> = s.split('-').collect();
                parts.len() == 5
                    && parts[0].len() == 8
                    && parts[1].len() == 4
                    && parts[2].len() == 4
                    && parts[3].len() == 4
                    && parts[4].len() == 12
                    && parts.iter().all(|p| p.chars().all(|c| c.is_ascii_hexdigit()))
            }),
            ("date-time", |s: &str| {
                let (date, time) = match s.split_once('T') {
                    Some((d, t)) => (d, t),
                    None => return false,
                };
                let ds: Vec<&str> = date.split('-').collect();
                if ds.len() != 3
                    || ds.iter().any(|p| p.is_empty() || !p.chars().all(|c| c.is_ascii_digit()))
                {
                    return false;
                }
                let time = time.trim_end_matches('Z').trim_end_matches(['+', '-']);
                let time = time.split('.').next().unwrap_or(time);
                let ts: Vec<&str> = time.split(':').collect();
                ts.len() >= 2
                    && ts.len() <= 3
                    && ts.iter().all(|p| {
                        p.len() <= 2 && !p.is_empty() && p.chars().all(|c| c.is_ascii_digit())
                    })
            }),
            ("date", |s: &str| {
                let ds: Vec<&str> = s.split('-').collect();
                ds.len() == 3
                    && ds.iter().all(|p| !p.is_empty() && p.chars().all(|c| c.is_ascii_digit()))
            }),
            ("hostname", |s: &str| {
                !s.is_empty()
                    && !s.starts_with('-')
                    && !s.ends_with('-')
                    && !s.contains("..")
                    && s.chars().all(|c| c.is_alphanumeric() || c == '-' || c == '.')
            }),
            ("ipv4", |s: &str| {
                let parts: Vec<&str> = s.split('.').collect();
                parts.len() == 4
                    && parts.iter().all(|p| {
                        p.len() <= 3
                            && !p.is_empty()
                            && p.chars().all(|c| c.is_ascii_digit())
                            && p.parse::<u8>().is_ok()
                    })
            }),
        ]
    })
}

/// Build a JSON Schema validator, registering the built-in `format` checkers
/// and asserting `format` regardless of draft default. This is the single
/// source of truth for both one-shot and precompiled validation in the
/// runtime. The validator is not cached here — callers that need caching
/// (routes) should use [`compile_schema`].
fn build_validator(
    schema_value: &serde_json::Value,
) -> Result<Validator, jsonschema::ValidationError<'static>> {
    let mut opts = jsonschema::options();
    opts = opts.should_validate_formats(true);
    for (name, checker) in format_validators() {
        opts = opts.with_format(*name, *checker);
    }
    opts.build(schema_value)
}

// ---------------------------------------------------------------------------
// Validation error types
// ---------------------------------------------------------------------------

/// A structured validation error with field-level messages (RFC 9457 format).
#[derive(Debug, Clone)]
pub struct ValidationError {
    pub errors: Vec<FieldError>,
}

impl ValidationError {
    pub fn new(field: impl Into<String>, message: impl Into<String>) -> Self {
        Self { errors: vec![FieldError { field: field.into(), message: message.into() }] }
    }

    pub fn multiple(errors: Vec<FieldError>) -> Self {
        Self { errors }
    }

    pub fn add(&mut self, field: impl Into<String>, message: impl Into<String>) {
        self.errors.push(FieldError { field: field.into(), message: message.into() });
    }

    pub fn is_empty(&self) -> bool {
        self.errors.is_empty()
    }
}

#[derive(Debug, Clone)]
pub struct FieldError {
    pub field: String,
    pub message: String,
}

impl fmt::Display for ValidationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (i, err) in self.errors.iter().enumerate() {
            if i > 0 {
                write!(f, "; ")?;
            }
            write!(f, "{}: {}", err.field, err.message)?;
        }
        Ok(())
    }
}

impl std::error::Error for ValidationError {}

/// Build an RFC 9457 Problem Details response for validation failures (422).
pub fn validation_error_response(err: &ValidationError) -> Response<ResponseBody> {
    let body = serde_json::json!({
        "type": "https://justapi.dev/errors/validation",
        "title": "Validation Error",
        "status": 422,
        "detail": err.to_string(),
        "errors": err.errors.iter().map(|e| {
            serde_json::json!({
                "field": e.field,
                "message": e.message,
            })
        }).collect::<Vec<_>>(),
    })
    .to_string();

    Response::builder()
        .status(StatusCode::UNPROCESSABLE_ENTITY)
        .header("content-type", "application/problem+json")
        .header("content-length", body.len().to_string())
        .body(crate::UnsyncBoxBody::new(
            Full::new(Bytes::from(body))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .expect("Response::builder with valid inputs should never fail")
}

// ---------------------------------------------------------------------------
// Schema trait
// ---------------------------------------------------------------------------

/// A type that can be parsed from raw HTTP data and validated.
///
/// Automatically blanket-implemented for any `DeserializeOwned + Send + Sync + 'static`.
pub trait Schema: DeserializeOwned + Send + Sync + 'static {
    /// Validate the parsed value. Returns `Ok(())` by default.
    fn validate(&self) -> Result<(), ValidationError> {
        Ok(())
    }

    /// Parse from a JSON byte slice, then validate.
    fn parse_body(body: &[u8]) -> Result<Self, ValidationError> {
        let value: Self = serde_json::from_slice(body)
            .map_err(|e| ValidationError::new("body", format!("Invalid JSON: {}", e)))?;
        value.validate()?;
        Ok(value)
    }
}

/// Blanket implementation: any `DeserializeOwned + Send + Sync + 'static` is a Schema.
impl<T: DeserializeOwned + Send + Sync + 'static> Schema for T {}

// ---------------------------------------------------------------------------
// Type coercion (string → T)
// ---------------------------------------------------------------------------

/// Trait for types that can be coerced from a URL path/query parameter string.
pub trait Coerce: Sized {
    fn coerce(s: &str) -> Result<Self, ValidationError>;
}

macro_rules! impl_coerce_via_from_str {
    ($($t:ty),* $(,)?) => {
        $(
            impl Coerce for $t {
                fn coerce(s: &str) -> Result<Self, ValidationError> {
                    s.parse().map_err(|_| {
                        ValidationError::new("", format!("invalid value: expected {}, got '{}'", stringify!($t), s))
                    })
                }
            }
        )*
    };
}

impl_coerce_via_from_str!(String, i8, i16, i32, i64, i128, u8, u16, u32, u64, u128, f32, f64, bool);

impl Coerce for char {
    fn coerce(s: &str) -> Result<Self, ValidationError> {
        let mut chars = s.chars();
        match (chars.next(), chars.next()) {
            (Some(c), None) => Ok(c),
            _ => Err(ValidationError::new(
                "",
                format!("invalid value: expected a single character, got '{}'", s),
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Query parameter parsing
// ---------------------------------------------------------------------------

/// Parse query string into a typed struct using serde.
///
/// Supports `?key=value&key2=value2` format with URL-encoding.
/// Handles nested structs, optional fields, and type coercion.
pub fn parse_query<T: DeserializeOwned>(query: &str) -> Result<T, ValidationError> {
    let cleaned = query.trim_start_matches('?');
    serde_urlencoded::from_str(cleaned)
        .map_err(|e| ValidationError::new("query", format!("parse error: {}", e)))
}

// ---------------------------------------------------------------------------
// JSON Schema validation
// ---------------------------------------------------------------------------

/// Validate a JSON byte slice against a JSON Schema string.
///
/// Returns `ValidationError` with field-level errors on failure.
/// The schema should be a valid JSON Schema (Draft 2020-12 or earlier).
pub fn validate_json_schema(body: &[u8], schema_json: &str) -> Result<(), ValidationError> {
    let body_value: serde_json::Value = serde_json::from_slice(body)
        .map_err(|e| ValidationError::new("body", format!("Invalid JSON: {}", e)))?;

    let schema_value: serde_json::Value = serde_json::from_str(schema_json)
        .map_err(|e| ValidationError::new("schema", format!("Invalid schema JSON: {}", e)))?;

    let validator = build_validator(&schema_value)
        .map_err(|e| ValidationError::new("schema", format!("Schema compilation error: {}", e)))?;

    let mut verr = ValidationError { errors: Vec::new() };
    for error in validator.iter_errors(&body_value) {
        let path = error.instance_path().to_string();
        let field = if path.is_empty() || path == "/" || path == "#" {
            "body".to_string()
        } else {
            path.trim_start_matches('/').to_string()
        };
        verr.add(field, error.to_string());
    }
    if verr.is_empty() {
        Ok(())
    } else {
        Err(verr)
    }
}

// ---------------------------------------------------------------------------
// Precompiled schema validator (cacheable per route)
// ---------------------------------------------------------------------------

/// A JSON Schema compiled once and reused across requests.
///
/// `jsonschema::Validator` is `Send + Sync + Clone`, so a `CompiledValidator`
/// is safe to store in an `Arc<Vec<Option<CompiledValidator>>>` shared across
/// all handler threads — this avoids re-parsing *and* re-compiling the schema
/// on every request (the dominant per-request cost of [`validate_json_schema`]).
#[derive(Clone)]
pub struct CompiledValidator(pub Validator);

impl CompiledValidator {
    /// Validate a JSON body against the precompiled schema.
    pub fn validate(&self, body: &[u8]) -> Result<(), ValidationError> {
        let body_value: serde_json::Value = serde_json::from_slice(body)
            .map_err(|e| ValidationError::new("body", format!("Invalid JSON: {}", e)))?;
        let mut verr = ValidationError { errors: Vec::new() };
        for error in self.0.iter_errors(&body_value) {
            let path = error.instance_path().to_string();
            let field = if path.is_empty() || path == "/" || path == "#" {
                "body".to_string()
            } else {
                path.trim_start_matches('/').to_string()
            };
            verr.add(field, error.to_string());
        }
        if verr.is_empty() {
            Ok(())
        } else {
            Err(verr)
        }
    }
}

/// Compile a JSON Schema string once into a reusable [`CompiledValidator`].
pub fn compile_schema(schema_json: &str) -> Result<CompiledValidator, ValidationError> {
    let schema_value: serde_json::Value = serde_json::from_str(schema_json)
        .map_err(|e| ValidationError::new("schema", format!("Invalid schema JSON: {}", e)))?;
    let validator = build_validator(&schema_value)
        .map_err(|e| ValidationError::new("schema", format!("Schema compilation error: {}", e)))?;
    Ok(CompiledValidator(validator))
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Deserialize;

    #[derive(Debug, Deserialize, PartialEq)]
    struct UserParams {
        name: String,
        age: Option<i32>,
    }

    #[test]
    fn test_path_param_coerce_string() {
        assert_eq!(String::coerce("hello").unwrap(), "hello");
    }

    #[test]
    fn test_path_param_coerce_i32() {
        assert_eq!(i32::coerce("42").unwrap(), 42i32);
        assert!(i32::coerce("not_a_number").is_err());
    }

    #[test]
    fn test_path_param_coerce_i64() {
        assert_eq!(i64::coerce("9999999999").unwrap(), 9_999_999_999i64);
    }

    #[test]
    fn test_path_param_coerce_f64() {
        assert_eq!(f64::coerce("2.5").unwrap(), 2.5_f64);
    }

    #[test]
    fn test_path_param_coerce_bool() {
        assert!(bool::coerce("true").unwrap());
        assert!(!bool::coerce("false").unwrap());
        assert!(bool::coerce("invalid").is_err());
    }

    #[test]
    fn test_schema_parse_valid_json() {
        let json = br#"{"name": "Alice", "age": 30}"#;
        let params: UserParams = Schema::parse_body(json).unwrap();
        assert_eq!(params.name, "Alice");
        assert_eq!(params.age, Some(30));
    }

    #[test]
    fn test_schema_parse_invalid_json() {
        let json = b"not json";
        let result: Result<UserParams, ValidationError> = Schema::parse_body(json);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_validation_error_display() {
        let err = ValidationError::multiple(vec![
            FieldError { field: "name".into(), message: "required".into() },
            FieldError { field: "age".into(), message: "must be positive".into() },
        ]);
        let msg = err.to_string();
        assert!(msg.contains("name: required"));
        assert!(msg.contains("age: must be positive"));
    }

    #[test]
    fn test_validation_error_response_format() {
        let err = ValidationError::new("email", "invalid email address");
        let resp = validation_error_response(&err);
        assert_eq!(resp.status(), StatusCode::UNPROCESSABLE_ENTITY);
        let content_type = resp.headers().get("content-type").unwrap();
        assert_eq!(content_type, "application/problem+json");
    }

    #[test]
    fn test_query_parse_simple() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Simple {
            name: String,
            age: i32,
        }
        let result: Simple = parse_query("name=Alice&age=30").unwrap();
        assert_eq!(result.name, "Alice");
        assert_eq!(result.age, 30);
    }

    #[test]
    fn test_query_parse_percent_encoded() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct Q {
            name: String,
        }
        let result: Q = parse_query("name=Hello%20World").unwrap();
        assert_eq!(result.name, "Hello World");
    }

    #[test]
    fn test_query_parse_optional_fields() {
        #[derive(Debug, Deserialize, PartialEq)]
        struct WithOptional {
            required: String,
            optional: Option<String>,
        }
        let result: WithOptional = parse_query("required=hello").unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, None);

        let result: WithOptional = parse_query("required=hello&optional=world").unwrap();
        assert_eq!(result.required, "hello");
        assert_eq!(result.optional, Some("world".into()));
    }

    #[test]
    fn test_validate_json_schema_valid() {
        let body = br#"{"name": "Alice", "email": "a@b.com"}"#;
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "email": {"type": "string"}}, "required": ["name", "email"], "additionalProperties": false}"#;
        assert!(validate_json_schema(body, schema).is_ok());
    }

    #[test]
    fn test_validate_json_schema_missing_field() {
        let body = br#"{"name": "Alice"}"#;
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "email": {"type": "string"}}, "required": ["name", "email"]}"#;
        let result = validate_json_schema(body, schema);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("email"));
        assert!(err.to_string().contains("required"));
    }

    #[test]
    fn test_validate_json_schema_wrong_type() {
        let body = br#"{"name": "Alice", "email": "a@b.com", "age": "not a number"}"#;
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}, "email": {"type": "string"}, "age": {"type": "integer"}}, "required": ["name", "email"]}"#;
        let result = validate_json_schema(body, schema);
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert!(err.to_string().contains("not of type"));
    }

    #[test]
    fn test_validate_json_schema_invalid_body() {
        let body = b"not json";
        let schema = r#"{"type": "object"}"#;
        let result = validate_json_schema(body, schema);
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Invalid JSON"));
    }

    #[test]
    fn test_compiled_validator_reuse() {
        let schema = r#"{"type": "object", "properties": {"name": {"type": "string"}}, "required": ["name"]}"#;
        let compiled = compile_schema(schema).unwrap();
        // Reused across many requests without recompiling.
        for _ in 0..1000 {
            assert!(compiled.validate(br#"{"name": "Alice"}"#).is_ok());
            assert!(compiled.validate(br#"{"wrong": 1}"#).is_err());
        }
    }

    // --- Crash-prevention tests (Phase 53.3) ---

    #[test]
    fn test_email_format_empty_string_no_panic() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        assert!(!email_checker.1(""));
    }

    #[test]
    fn test_email_format_no_at_sign_no_panic() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        assert!(!email_checker.1("notanemail"));
    }

    #[test]
    fn test_email_format_at_dot_prefix_no_panic() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        assert!(!email_checker.1("@example.com"));
    }

    #[test]
    fn test_email_format_whitespace_no_panic() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        assert!(!email_checker.1("user @example.com"));
    }

    #[test]
    fn test_email_format_multiple_at_no_panic() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        // "a@b@c" — split_once only splits on first @, local="a", domain="b@c"
        // domain contains no '.' so it's invalid, but must not panic
        assert!(!email_checker.1("a@b@c"));
    }

    #[test]
    fn test_email_format_valid() {
        let validators = super::format_validators();
        let email_checker = validators.iter().find(|(name, _)| *name == "email").unwrap();
        assert!(email_checker.1("user@example.com"));
        assert!(email_checker.1("a+b@domain.co.uk"));
    }

    #[test]
    fn test_validate_json_schema_with_email_format_no_panic() {
        let body = br#"{"email": "not-an-email"}"#;
        let schema = r#"{"type": "object", "properties": {"email": {"type": "string", "format": "email"}}, "required": ["email"]}"#;
        // Must return a validation error, not panic
        let result = validate_json_schema(body, schema);
        assert!(result.is_err());
    }
}
