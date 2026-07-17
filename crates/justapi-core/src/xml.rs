//! XML request/response support for JustAPI.
//!
//! XML is a first-class content type alongside JSON:
//! - **Responses:** [`xml_response`] (raw string) and [`XmlResponse`] (typed,
//!   mirrors [`crate::serialize::JsonResponse`]) emit `application/xml`.
//! - **Requests:** [`xml_to_json`] normalizes an `application/xml` / `text/xml`
//!   body into a `serde_json::Value` so the rest of the pipeline (which speaks
//!   JSON `Value`) can consume XML input transparently. This keeps the Rust
//!   parsing in `justapi-core` (per ADR-008) and lets Python handlers receive a
//!   JSON-shaped body regardless of the wire format.
//! - **Content negotiation:** [`Format`] + [`negotiate`] + [`respond`] let a
//!   handler return JSON or XML based on the request `Accept` / `Content-Type`.
//!
//! Powered by `quick-xml` (serde feature). See ADR-064.

use anyhow::{anyhow, Result};
use bytes::Bytes;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::header::HeaderName;
use hyper::{Response, StatusCode};

use crate::ResponseBody;

/// The two negotiated wire formats.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Format {
    Json,
    Xml,
}

impl Format {
    /// The `content-type` for this format.
    pub fn content_type(self) -> &'static str {
        match self {
            Format::Json => "application/json",
            Format::Xml => "application/xml",
        }
    }
}

/// Decide the response format from a request's `Content-Type` (input) and
/// `Accept` (output preference). `Accept: application/xml` (or `text/xml`)
/// selects XML; everything else (including `*/*`, missing, or JSON) selects
/// JSON. An incoming XML body does **not** force an XML response — output is
/// driven by `Accept`.
pub fn negotiate(content_type: Option<&str>, accept: Option<&str>) -> Format {
    let accept = accept.unwrap_or("").to_ascii_lowercase();
    if accept.contains("application/xml") || accept.contains("text/xml") {
        return Format::Xml;
    }
    // If the client sent XML and did not ask for anything specific, echo XML.
    let ct = content_type.unwrap_or("").to_ascii_lowercase();
    if accept.is_empty() && (ct.contains("application/xml") || ct.contains("text/xml")) {
        return Format::Xml;
    }
    Format::Json
}

// ---------------------------------------------------------------------------
// Low-level helpers
// ---------------------------------------------------------------------------

/// Lift a raw XML string into an `application/xml` response.
pub fn xml_response(status: StatusCode, body: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/xml")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(
            Full::new(Bytes::from(body.to_string()))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

/// Serialize a `Serialize` value to an XML string with the given root element.
pub fn to_xml_string<T: serde::Serialize>(root: &str, value: &T) -> Result<String> {
    quick_xml::se::to_string_with_root(root, value)
        .map_err(|e| anyhow!("xml serialization failed: {e}"))
}

/// Serialize a `serde_json::Value` to XML using `root` as the document element.
pub fn json_to_xml(root: &str, value: &serde_json::Value) -> Result<String> {
    to_xml_string(root, value)
}

/// Parse an XML byte slice into a `serde_json::Value`. Element-centric: nested
/// elements become nested objects, repeated sibling elements with the same tag
/// become arrays, text content becomes a string (or number when it parses as
/// one). XML **attributes** are captured under `@name` keys; mixed
/// element/text content places the text under `#text`.
///
/// This is a small, dependency-light converter built on quick-xml's event
/// reader (not its serde `Deserializer`, which cannot represent open JSON
/// `Value`s). It covers the common document shapes; namespaces and
/// comments are ignored.
/// A frame on the XML→JSON conversion stack: an in-progress object (element
/// with children/attrs). `name` is the element's tag, used as the key when
/// inserting this frame into its parent. Repeated sibling elements are folded
/// into arrays by `insert_child`, so no separate array frame is needed.
enum Frame {
    Object { name: String, obj: serde_json::Map<String, serde_json::Value> },
}

pub fn xml_to_json(bytes: &[u8]) -> Result<serde_json::Value> {
    use quick_xml::events::Event;
    use quick_xml::reader::Reader;

    let mut reader = Reader::from_str(std::str::from_utf8(bytes)?);
    reader.config_mut().trim_text(true);
    let mut buf = Vec::new();

    // Stack of in-progress containers. Each frame is either an object (Map) or
    // an array (Vec), plus the current text buffer for the innermost element.
    let mut stack: Vec<Frame> = Vec::new();
    // Pending text buffer for the current element.
    let mut text_buf = String::new();

    // Helper: flush accumulated text into the parent as a value or #text.
    macro_rules! flush_text {
        () => {{
            let txt = text_buf.trim().to_string();
            text_buf.clear();
            txt
        }};
    }

    loop {
        match reader.read_event_into(&mut buf) {
            Ok(Event::Start(e)) => {
                let name = String::from_utf8_lossy(e.name().into_inner()).into_owned();
                // Close any pending text on the parent before descending.
                let _ = flush_text!();
                let mut obj = serde_json::Map::new();
                // Capture attributes as @key entries.
                for attr in e.attributes().flatten() {
                    let an = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let av = String::from_utf8_lossy(&attr.value).into_owned();
                    obj.insert(format!("@{an}"), coerce(av));
                }
                stack.push(Frame::Object { name, obj });
                text_buf.clear();
            }
            Ok(Event::Empty(e)) => {
                let name = String::from_utf8_lossy(e.name().into_inner()).into_owned();
                let mut obj = serde_json::Map::new();
                for attr in e.attributes().flatten() {
                    let an = String::from_utf8_lossy(attr.key.as_ref()).into_owned();
                    let av = String::from_utf8_lossy(&attr.value).into_owned();
                    obj.insert(format!("@{an}"), coerce(av));
                }
                let val = if obj.is_empty() {
                    serde_json::Value::Null
                } else {
                    serde_json::Value::Object(obj)
                };
                insert_child(&mut stack, &name, val);
            }
            Ok(Event::Text(e)) => {
                let t = e.unescape().unwrap_or_default();
                text_buf.push_str(&t);
            }
            Ok(Event::CData(e)) => {
                text_buf.push_str(&String::from_utf8_lossy(e.as_ref()));
            }
            Ok(Event::End(_)) => {
                // If this is the only frame on the stack, closing it means the
                // document root — build its value and return it directly.
                let is_root = stack.len() == 1;
                // Pop the current frame and attach it (or the buffered text) to
                // the parent under the frame's own tag name.
                let frame = stack.pop().expect("unbalanced xml");
                let (key, value) = match frame {
                    Frame::Object { name, mut obj } => {
                        let txt = flush_text!();
                        if !txt.is_empty() {
                            // Text alongside children/attrs (or a leaf element):
                            // keep the text under `#text`.
                            obj.insert("#text".to_string(), coerce(txt));
                        }
                        let value = if obj.is_empty() {
                            serde_json::Value::Null
                        } else if obj.len() == 1 && obj.contains_key("#text") {
                            // A leaf element with only text (no attributes, no
                            // child elements) collapses to the scalar text, so
                            // `<id>1</id>` becomes `{"id": 1}` rather than
                            // `{"id": {"#text": 1}}`.
                            obj.remove("#text").unwrap()
                        } else {
                            serde_json::Value::Object(obj)
                        };
                        (name, value)
                    }
                };
                if is_root {
                    // The document root is wrapped under its own tag name so the
                    // result is always `{ "root_tag": <content> }`.
                    let mut root = serde_json::Map::new();
                    root.insert(key, value);
                    return Ok(serde_json::Value::Object(root));
                }
                insert_child(&mut stack, &key, value);
            }
            Ok(Event::Eof) => break,
            Ok(Event::Comment(_) | Event::PI(_) | Event::Decl(_) | Event::DocType(_)) => {}
            Err(e) => return Err(anyhow!("xml parse error: {e}")),
        }
    }

    // Fallback: if the document was a bare text node.
    if stack.is_empty() {
        let t = text_buf.trim().to_string();
        if t.is_empty() {
            return Ok(serde_json::Value::Null);
        }
        return Ok(coerce(t));
    }
    Err(anyhow!("xml parse failed: incomplete document"))
}

/// Coerce a string to a JSON number when it parses, else keep it a string.
fn coerce(s: String) -> serde_json::Value {
    if let Ok(n) = s.parse::<i64>() {
        return serde_json::Value::Number(n.into());
    }
    if let Ok(f) = s.parse::<f64>() {
        if let Some(n) = serde_json::Number::from_f64(f) {
            return serde_json::Value::Number(n);
        }
    }
    serde_json::Value::String(s)
}

/// Insert `value` under `key` into the top frame of the stack. If the top frame
/// is an object that already has `key`, promote it to an array (repeated
/// sibling elements).
fn insert_child(stack: &mut Vec<Frame>, key: &str, value: serde_json::Value) {
    let top = match stack.last_mut() {
        Some(t) => t,
        None => {
            // No parent: this is the document root with no enclosing frame yet.
            // We synthesize a single-element object so the result is always an
            // object keyed by the root tag.
            let mut m = serde_json::Map::new();
            m.insert(key.to_string(), value);
            stack.push(Frame::Object { name: key.to_string(), obj: m });
            return;
        }
    };
    match top {
        Frame::Object { obj, .. } => {
            if let Some(existing) = obj.remove(key) {
                let arr = match existing {
                    serde_json::Value::Array(mut a) => {
                        a.push(value);
                        a
                    }
                    other => vec![other, value],
                };
                obj.insert(key.to_string(), serde_json::Value::Array(arr));
            } else {
                obj.insert(key.to_string(), value);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Typed response model
// ---------------------------------------------------------------------------

/// An XML response shaped from any `Serialize` type. Mirrors
/// [`crate::serialize::JsonResponse`] but emits `application/xml`. The caller
/// supplies the root element name (XML documents require exactly one root).
///
/// # Example
///
/// ```ignore
/// #[derive(Serialize)]
/// struct User { id: u64, name: String }
///
/// let resp = XmlResponse::new("user", User { id: 1, name: "Alice".into() })
///     .with_status(StatusCode::CREATED)
///     .into_response();
/// ```
pub struct XmlResponse<T: serde::Serialize> {
    root: &'static str,
    data: T,
    status: StatusCode,
    headers: Vec<(HeaderName, String)>,
}

impl<T: serde::Serialize> XmlResponse<T> {
    /// Create a new `XmlResponse` with the given root element and status 200.
    pub fn new(root: &'static str, data: T) -> Self {
        Self { root, data, status: StatusCode::OK, headers: Vec::new() }
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
        let body_str = to_xml_string(self.root, &self.data)?;
        let body_bytes = body_str.into_bytes();
        let mut builder = Response::builder().status(self.status);
        builder = builder.header("content-type", "application/xml");
        builder = builder.header("content-length", body_bytes.len().to_string());
        for (name, value) in &self.headers {
            builder = builder.header(name, value);
        }
        let resp = builder
            .body(UnsyncBoxBody::new(
                Full::new(Bytes::from(body_bytes))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))
            .unwrap();
        Ok(resp)
    }
}

/// Convenience: build an `XmlResponse` from any `Serialize` value.
pub fn xml_response_from<T: serde::Serialize>(root: &'static str, data: T) -> XmlResponse<T> {
    XmlResponse::new(root, data)
}

// ---------------------------------------------------------------------------
// Content-negotiated response
// ---------------------------------------------------------------------------

/// Produce a response in the negotiated `format` from a `serde_json::Value`.
/// JSON emits compact JSON; XML wraps the value under the given `root` element.
pub fn respond(
    status: StatusCode,
    root: &str,
    value: &serde_json::Value,
    format: Format,
) -> Result<Response<ResponseBody>> {
    match format {
        Format::Json => {
            let body = serde_json::to_vec(value)?;
            Ok(Response::builder()
                .status(status)
                .header("content-type", "application/json")
                .header("content-length", body.len().to_string())
                .body(UnsyncBoxBody::new(
                    Full::new(Bytes::from(body))
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap())
        }
        Format::Xml => Ok(xml_response(status, &json_to_xml(root, value)?)),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde::Serialize;

    #[derive(Serialize)]
    struct User {
        id: u64,
        name: String,
    }

    #[test]
    fn test_xml_response_raw() {
        let resp = xml_response(StatusCode::OK, "<note>hi</note>");
        assert_eq!(resp.headers()["content-type"], "application/xml");
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_xml_response_typed() {
        let user = User { id: 1, name: "Alice".into() };
        let resp = XmlResponse::new("user", user).into_response().unwrap();
        assert_eq!(resp.headers()["content-type"], "application/xml");
        assert_eq!(resp.status(), StatusCode::OK);
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        let xml = String::from_utf8(body.to_vec()).unwrap();
        assert!(xml.contains("<user>"));
        assert!(xml.contains("<id>1</id>"));
        assert!(xml.contains("<name>Alice</name>"));
    }

    #[test]
    fn test_xml_response_status_and_headers() {
        let user = User { id: 2, name: "Bob".into() };
        let resp = XmlResponse::new("user", user)
            .with_status(StatusCode::CREATED)
            .with_header("x-request-id", "r-1")
            .into_response()
            .unwrap();
        assert_eq!(resp.status(), StatusCode::CREATED);
        assert_eq!(resp.headers()["x-request-id"], "r-1");
    }

    #[test]
    fn test_json_to_xml() {
        let v = serde_json::json!({"id": 1, "name": "Alice"});
        let xml = json_to_xml("user", &v).unwrap();
        assert!(xml.contains("<user>"));
        assert!(xml.contains("<id>1</id>"));
        assert!(xml.contains("<name>Alice</name>"));
    }

    #[test]
    fn test_xml_to_json() {
        let xml = r#"<user><id>1</id><name>Alice</name></user>"#;
        let v = xml_to_json(xml.as_bytes()).unwrap();
        assert_eq!(v["user"]["id"], 1);
        assert_eq!(v["user"]["name"], "Alice");
    }

    #[test]
    fn test_xml_to_json_array() {
        let xml = r#"<root><item>a</item><item>b</item></root>"#;
        let v = xml_to_json(xml.as_bytes()).unwrap();
        assert_eq!(v["root"]["item"][0], "a");
        assert_eq!(v["root"]["item"][1], "b");
    }

    #[test]
    fn test_negotiate() {
        assert_eq!(negotiate(None, Some("application/xml")), Format::Xml);
        assert_eq!(negotiate(None, Some("text/xml")), Format::Xml);
        assert_eq!(negotiate(None, Some("application/json")), Format::Json);
        assert_eq!(negotiate(None, Some("*/*")), Format::Json);
        assert_eq!(negotiate(None, None), Format::Json);
        // Incoming XML with no Accept preference echoes XML.
        assert_eq!(negotiate(Some("application/xml"), None), Format::Xml);
        assert_eq!(negotiate(Some("application/json"), None), Format::Json);
    }

    #[tokio::test]
    async fn test_respond_content_negotiated() {
        let v = serde_json::json!({"id": 1});
        let j = respond(StatusCode::OK, "user", &v, Format::Json).unwrap();
        assert_eq!(j.headers()["content-type"], "application/json");
        let x = respond(StatusCode::OK, "user", &v, Format::Xml).unwrap();
        assert_eq!(x.headers()["content-type"], "application/xml");
        let body = x.into_body().collect().await.unwrap().to_bytes();
        assert!(String::from_utf8(body.to_vec()).unwrap().contains("<user>"));
    }
}
