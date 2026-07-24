use std::collections::HashMap;
use std::str::FromStr;

use anyhow::Result;
use bytes::Bytes;
use http_body_util::BodyExt;
use hyper::Request;
use serde::de::DeserializeOwned;

// ---------------------------------------------------------------------------
// Typed form data (application/x-www-form-urlencoded)
// ---------------------------------------------------------------------------

/// Typed form data extracted from an `application/x-www-form-urlencoded` body.
///
/// # Example
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct Login { username: String, password: String }
///
/// async fn handler(req: Request<Incoming>) -> Result<Response<ResponseBody>> {
///     let form = Form::<Login>::from_request(req).await?;
///     Ok(json_response(StatusCode::OK, &form.username))
/// }
/// ```
pub struct Form<T>(pub T);

impl<T: DeserializeOwned> Form<T> {
    /// Parse from raw `application/x-www-form-urlencoded` bytes.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let value: T = serde_urlencoded::from_bytes(bytes)
            .map_err(|e| anyhow::anyhow!("form error: {}", e))?;
        Ok(Form(value))
    }

    /// Parse from a request. Consumes the request body. Content-Type is NOT
    /// checked here — callers should verify it themselves or use the
    /// middleware-level guard.
    pub async fn from_request<B>(req: Request<B>) -> Result<Self>
    where
        B: http_body::Body<Data = Bytes> + Send + Unpin + 'static,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let (_parts, body) = req.into_parts();
        let bytes = body.collect().await.map_err(|e| anyhow::anyhow!(e))?.to_bytes();
        Self::from_bytes(&bytes)
    }
}

// ---------------------------------------------------------------------------
// Typed query parameters
// ---------------------------------------------------------------------------

/// Typed query parameters extracted from the request URL.
///
/// # Example
///
/// ```ignore
/// #[derive(Deserialize)]
/// struct Page { page: Option<u32>, limit: Option<u32> }
///
/// fn handler(req: Request<Incoming>) -> ... {
///     let q = Query::<Page>::from_request(&req)?;
/// }
/// ```
pub struct Query<T>(pub T);

impl<T: DeserializeOwned> Query<T> {
    /// Parse from a raw query string (without the leading `?`).
    pub fn parse(query: &str) -> Result<Self> {
        let cleaned = query.trim_start_matches('?');
        let value: T = serde_urlencoded::from_str(cleaned)
            .map_err(|e| anyhow::anyhow!("query error: {}", e))?;
        Ok(Query(value))
    }

    /// Parse from a request's URI query string.
    pub fn from_request<B>(req: &Request<B>) -> Result<Self> {
        Self::parse(req.uri().query().unwrap_or(""))
    }
}

// ---------------------------------------------------------------------------
// Cookie jar
// ---------------------------------------------------------------------------

/// Parsed cookies from the `Cookie` request header.
///
/// # Example
///
/// ```ignore
/// fn handler(req: Request<Incoming>) -> ... {
///     let jar = CookieJar::from_request(&req);
///     if let Some(session) = jar.get("session_id") { ... }
/// }
/// ```
pub struct CookieJar {
    cookies: HashMap<String, String>,
}

impl CookieJar {
    /// Parse the `Cookie` header into a key-value map.
    pub fn from_request<B>(req: &Request<B>) -> Self {
        let mut cookies = HashMap::new();
        if let Some(header) = req.headers().get("cookie").and_then(|v| v.to_str().ok()) {
            for pair in header.split(';') {
                let pair = pair.trim();
                if let Some((name, value)) = pair.split_once('=') {
                    cookies.insert(url_decode(name.trim()), url_decode(value.trim()));
                }
            }
        }
        CookieJar { cookies }
    }

    pub fn get(&self, name: &str) -> Option<&str> {
        self.cookies.get(name).map(|s| s.as_str())
    }

    pub fn is_empty(&self) -> bool {
        self.cookies.is_empty()
    }

    pub fn len(&self) -> usize {
        self.cookies.len()
    }

    pub fn iter(&self) -> impl Iterator<Item = (&str, &str)> {
        self.cookies.iter().map(|(k, v)| (k.as_str(), v.as_str()))
    }
}

// ---------------------------------------------------------------------------
// Header extraction helpers
// ---------------------------------------------------------------------------

/// Access a header value as a string.
pub fn get_header<'a, B>(req: &'a Request<B>, name: &str) -> Option<&'a str> {
    let header_name = hyper::header::HeaderName::from_bytes(name.as_bytes()).ok()?;
    req.headers().get(&header_name)?.to_str().ok()
}

/// Access a header value parsed into a specific type.
pub fn get_header_typed<T: FromStr, B>(req: &Request<B>, name: &str) -> Result<Option<T>>
where
    T::Err: std::fmt::Display,
{
    match get_header(req, name) {
        Some(val) => val
            .parse::<T>()
            .map(Some)
            .map_err(|e| anyhow::anyhow!("invalid header '{}': {}", name, e)),
        None => Ok(None),
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/// Minimal URL-decoding (percent-encoding and '+') for cookie values.
/// Processes bytes, not chars, to correctly decode multi-byte UTF-8 sequences.
fn url_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut result = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16).unwrap_or(0);
            let lo = (bytes[i + 2] as char).to_digit(16).unwrap_or(0);
            result.push(hi as u8 * 16 + lo as u8);
            i += 3;
        } else if bytes[i] == b'+' {
            result.push(b' ');
            i += 1;
        } else {
            result.push(bytes[i]);
            i += 1;
        }
    }
    String::from_utf8(result).unwrap_or_else(|e| String::from_utf8_lossy(e.as_bytes()).into_owned())
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use hyper::Method;
    use serde::Deserialize;

    type TestBody = http_body_util::Full<Bytes>;

    fn test_req(method: Method, uri: &str) -> Request<TestBody> {
        Request::builder().method(method).uri(uri).body(TestBody::new(Bytes::new())).unwrap()
    }

    fn form_req(body: &str) -> Request<TestBody> {
        Request::builder()
            .method(Method::POST)
            .uri("/submit")
            .header("content-type", "application/x-www-form-urlencoded")
            .body(TestBody::new(Bytes::from(body.to_string())))
            .unwrap()
    }

    // --- Form tests ---

    #[derive(Deserialize, Debug, PartialEq)]
    struct LoginForm {
        username: String,
        password: String,
    }

    #[test]
    fn form_from_bytes_parses() {
        let form = Form::<LoginForm>::from_bytes(b"username=alice&password=secret").unwrap();
        assert_eq!(form.0.username, "alice");
        assert_eq!(form.0.password, "secret");
    }

    #[test]
    fn form_from_bytes_missing_field_errors() {
        let result = Form::<LoginForm>::from_bytes(b"username=alice");
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn form_from_request_parses() {
        let req = form_req("username=bob&password=pass");
        let form = Form::<LoginForm>::from_request(req).await.unwrap();
        assert_eq!(form.0.username, "bob");
    }

    #[tokio::test]
    async fn form_from_request_empty_errors() {
        let req = form_req("");
        let result = Form::<LoginForm>::from_request(req).await;
        assert!(result.is_err());
    }

    // --- Query tests ---

    #[derive(Deserialize, Debug, PartialEq)]
    struct Pagination {
        page: Option<u32>,
        limit: Option<u32>,
    }

    #[test]
    fn query_parse_parses() {
        let q = Query::<Pagination>::parse("page=1&limit=20").unwrap();
        assert_eq!(q.0.page, Some(1));
        assert_eq!(q.0.limit, Some(20));
    }

    #[test]
    fn query_from_request_parses() {
        let req = test_req(Method::GET, "/items?page=3&limit=10");
        let q = Query::<Pagination>::from_request(&req).unwrap();
        assert_eq!(q.0.page, Some(3));
    }

    #[test]
    fn query_empty_returns_defaults() {
        let req = test_req(Method::GET, "/items");
        let q = Query::<Pagination>::from_request(&req).unwrap();
        assert_eq!(q.0.page, None);
        assert_eq!(q.0.limit, None);
    }

    #[test]
    fn query_parse_with_leading_qmark() {
        let q = Query::<Pagination>::parse("?page=5").unwrap();
        assert_eq!(q.0.page, Some(5));
    }

    // --- CookieJar tests ---

    #[test]
    fn cookies_single() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("cookie", "session_id=abc123".parse().unwrap());
        let jar = CookieJar::from_request(&req);
        assert_eq!(jar.get("session_id"), Some("abc123"));
    }

    #[test]
    fn cookies_multiple() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("cookie", "session=xyz; theme=dark; lang=en-US".parse().unwrap());
        let jar = CookieJar::from_request(&req);
        assert_eq!(jar.get("session"), Some("xyz"));
        assert_eq!(jar.get("theme"), Some("dark"));
        assert_eq!(jar.get("lang"), Some("en-US"));
        assert_eq!(jar.len(), 3);
    }

    #[test]
    fn cookies_no_header_empty() {
        let req = test_req(Method::GET, "/");
        let jar = CookieJar::from_request(&req);
        assert!(jar.is_empty());
    }

    #[test]
    fn cookies_url_decoded() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("cookie", "name=Hello%20World".parse().unwrap());
        let jar = CookieJar::from_request(&req);
        assert_eq!(jar.get("name"), Some("Hello World"));
    }

    #[test]
    fn cookies_plus_decoded() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("cookie", "q=rust+lang".parse().unwrap());
        let jar = CookieJar::from_request(&req);
        assert_eq!(jar.get("q"), Some("rust lang"));
    }

    // --- Header helpers ---

    #[test]
    fn header_value_present() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("x-api-key", "secret".parse().unwrap());
        assert_eq!(get_header(&req, "x-api-key"), Some("secret"));
    }

    #[test]
    fn header_value_missing() {
        let req = test_req(Method::GET, "/");
        assert_eq!(get_header(&req, "x-api-key"), None);
    }

    #[test]
    fn header_typed_parses() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("x-count", "42".parse().unwrap());
        let val: Option<u32> = get_header_typed(&req, "x-count").unwrap();
        assert_eq!(val, Some(42));
    }

    #[test]
    fn header_typed_invalid_errors() {
        let mut req = test_req(Method::GET, "/");
        req.headers_mut().insert("x-count", "abc".parse().unwrap());
        let result: Result<Option<u32>> = get_header_typed(&req, "x-count");
        assert!(result.is_err());
    }
}
