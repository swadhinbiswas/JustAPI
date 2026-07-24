use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::Result;
use futures::StreamExt;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::{BodyExt, Full};
use hyper::body::{Bytes, Frame};
use hyper::{Request, Response, StatusCode};
use tokio::io::{AsyncReadExt, AsyncSeekExt};

use crate::ResponseBody;

/// A static-file mount: serves files from `dir` under `prefix`, with an
/// optional SPA fallback (e.g. `index.html`) for unmatched paths.
#[derive(Clone)]
pub struct StaticMount {
    pub prefix: String,
    pub dir: StaticDir,
    /// When set, unmatched paths fall back to this file (relative to `dir`).
    pub fallback: Option<String>,
}

impl StaticMount {
    /// Resolve a request path against this mount. Returns `None` if the path
    /// is not under `prefix`.
    pub fn resolve(&self, uri_path: &str) -> Option<PathBuf> {
        let rel = if self.prefix == "/" {
            uri_path.to_string()
        } else {
            if !uri_path.starts_with(&self.prefix) {
                return None;
            }
            uri_path[self.prefix.len()..].to_string()
        };
        self.dir.resolve(&rel)
    }
}

/// Serve static files from a root directory.
#[derive(Clone)]
pub struct StaticDir {
    root: Arc<PathBuf>,
}

impl StaticDir {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: Arc::new(root.into()) }
    }

    /// Return the configured root directory as a `PathBuf`.
    pub fn root(&self) -> PathBuf {
        (*self.root).clone()
    }

    /// Resolve a URI path to a file path, preventing directory traversal.
    pub fn resolve(&self, uri_path: &str) -> Option<PathBuf> {
        let relative = uri_path.trim_start_matches('/');
        if relative.is_empty() {
            return None;
        }
        // Block directory traversal: check both literal and percent-encoded `..`
        if relative.contains("..") || relative.contains("%2e") || relative.contains("%2E") {
            return None;
        }
        let path = self.root.join(relative);
        // Ensure the resolved path is still under root
        if path.starts_with(&*self.root) {
            Some(path)
        } else {
            None
        }
    }

    /// Generate an ETag from file metadata.
    fn make_etag(metadata: &std::fs::Metadata) -> String {
        format!(
            "\"{:x}-{:x}\"",
            metadata.len(),
            metadata
                .modified()
                .ok()
                .and_then(|t| t.elapsed().ok())
                .map(|d| d.as_secs())
                .unwrap_or(0)
        )
    }

    /// Serve a file at the given path. Returns a streaming response.
    /// Supports Range requests for partial content (HTTP 206).
    pub async fn serve_file(&self, path: &Path) -> Result<Response<ResponseBody>> {
        let metadata = tokio::fs::metadata(path).await?;
        if !metadata.is_file() {
            return not_found();
        }

        let etag = Self::make_etag(&metadata);
        let content_type = guess_content_type(path);
        let total_len = metadata.len();

        // For small files (< 1MB), read into memory for better compression support
        if total_len <= 1024 * 1024 {
            let bytes = tokio::fs::read(path).await?;
            let body = UnsyncBoxBody::new(
                http_body_util::Full::new(Bytes::from(bytes))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            );
            let mut resp = Response::new(body);
            *resp.status_mut() = StatusCode::OK;
            resp.headers_mut().insert("content-type", content_type.parse().unwrap());
            resp.headers_mut().insert("content-length", total_len.to_string().parse().unwrap());
            resp.headers_mut().insert("etag", etag.parse().unwrap());
            resp.headers_mut().insert("cache-control", "public, max-age=3600".parse().unwrap());
            return Ok(resp);
        }

        // For large files, use streaming
        let file = tokio::fs::File::open(path).await?;
        let stream = tokio_util::io::ReaderStream::new(file);
        let body_stream = stream.map(|r| r.map(Frame::data).map_err(|e| anyhow::anyhow!(e)));
        let body = UnsyncBoxBody::new(http_body_util::StreamBody::new(body_stream));

        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert("content-type", content_type.parse().unwrap());
        resp.headers_mut().insert("content-length", total_len.to_string().parse().unwrap());
        resp.headers_mut().insert("etag", etag.parse().unwrap());
        resp.headers_mut().insert("cache-control", "public, max-age=3600".parse().unwrap());

        Ok(resp)
    }

    /// Handle an HTTP request for a static file.
    /// Supports Range requests (HTTP 206), If-None-Match (HTTP 304),
    /// and proper cache headers.
    pub async fn handle(
        &self,
        req: Request<hyper::body::Incoming>,
    ) -> Result<Response<ResponseBody>> {
        let uri_path = req.uri().path().to_string();

        let path = match self.resolve(&uri_path) {
            Some(p) => p,
            None => return not_found(),
        };

        // ETag / If-None-Match check
        let metadata = match tokio::fs::metadata(&path).await {
            Ok(m) if m.is_file() => m,
            _ => return not_found(),
        };

        let etag = Self::make_etag(&metadata);

        // Conditional request: If-None-Match
        if let Some(if_none_match) = req.headers().get("if-none-match") {
            if let Ok(val) = if_none_match.to_str() {
                if val == etag {
                    return Ok(Response::builder()
                        .status(StatusCode::NOT_MODIFIED)
                        .header("etag", &etag)
                        .header("cache-control", "public, max-age=3600")
                        .body(UnsyncBoxBody::new(Full::new(Bytes::new()).map_err(
                            |e: std::convert::Infallible| -> anyhow::Error { match e {} },
                        )))
                        .unwrap());
                }
            }
        }

        let content_type = guess_content_type(&path);
        let total_len = metadata.len();

        // Range request support
        if let Some(range_header) = req.headers().get("range") {
            if let Ok(range_str) = range_header.to_str() {
                if let Some(range) = parse_range(range_str, total_len) {
                    return self.serve_range(&path, &etag, content_type, total_len, range).await;
                }
            }
        }

        // Full file response
        // For small files, read into memory (better for compression)
        if total_len <= 1024 * 1024 {
            let bytes = tokio::fs::read(&path).await?;
            let body = UnsyncBoxBody::new(
                http_body_util::Full::new(Bytes::from(bytes))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            );
            let mut resp = Response::new(body);
            *resp.status_mut() = StatusCode::OK;
            resp.headers_mut().insert("content-type", content_type.parse().unwrap());
            resp.headers_mut().insert("content-length", total_len.to_string().parse().unwrap());
            resp.headers_mut().insert("etag", etag.parse().unwrap());
            resp.headers_mut().insert("cache-control", "public, max-age=3600".parse().unwrap());
            resp.headers_mut().insert("accept-ranges", "bytes".parse().unwrap());
            return Ok(resp);
        }

        // Large files: stream
        let file = tokio::fs::File::open(&path).await?;
        let stream = tokio_util::io::ReaderStream::new(file);
        let body_stream = stream.map(|r| r.map(Frame::data).map_err(|e| anyhow::anyhow!(e)));
        let body = UnsyncBoxBody::new(http_body_util::StreamBody::new(body_stream));

        let mut resp = Response::new(body);
        *resp.status_mut() = StatusCode::OK;
        resp.headers_mut().insert("content-type", content_type.parse().unwrap());
        resp.headers_mut().insert("content-length", total_len.to_string().parse().unwrap());
        resp.headers_mut().insert("etag", etag.parse().unwrap());
        resp.headers_mut().insert("cache-control", "public, max-age=3600".parse().unwrap());
        resp.headers_mut().insert("accept-ranges", "bytes".parse().unwrap());

        Ok(resp)
    }

    /// Serve a range of bytes from a file (HTTP 206 Partial Content).
    async fn serve_range(
        &self,
        path: &Path,
        etag: &str,
        content_type: &str,
        total_len: u64,
        range: ByteRange,
    ) -> Result<Response<ResponseBody>> {
        let (start, end) = range.into_abs_range(total_len);
        let len = end - start + 1;

        let mut file = tokio::fs::File::open(path).await?;
        file.seek(std::io::SeekFrom::Start(start)).await?;

        let mut buf = vec![0u8; len as usize];
        file.read_exact(&mut buf).await?;

        let body = UnsyncBoxBody::new(
            http_body_util::Full::new(Bytes::from(buf))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        );

        let content_range = format!("bytes {}-{}/{}", start, end, total_len);

        let resp = Response::builder()
            .status(StatusCode::PARTIAL_CONTENT)
            .header("content-type", content_type)
            .header("content-length", len.to_string())
            .header("content-range", &content_range)
            .header("etag", etag)
            .header("accept-ranges", "bytes")
            .header("cache-control", "public, max-age=3600")
            .body(body)
            .unwrap();

        Ok(resp)
    }
}

/// Parsed byte range from a Range header.
pub(crate) enum ByteRange {
    /// bytes=start-end
    StartEnd(u64, u64),
    /// bytes=start-
    StartFrom(u64),
    /// bytes=-suffix
    Suffix(u64),
}

impl ByteRange {
    /// Convert to an absolute (start, end) range clamped to file size.
    pub(crate) fn into_abs_range(self, total_len: u64) -> (u64, u64) {
        match self {
            ByteRange::StartEnd(start, end) => {
                let start = start.min(total_len.saturating_sub(1));
                let end = end.min(total_len.saturating_sub(1));
                (start, end.max(start))
            }
            ByteRange::StartFrom(start) => {
                let start = start.min(total_len.saturating_sub(1));
                (start, total_len.saturating_sub(1))
            }
            ByteRange::Suffix(len) => {
                let start = total_len.saturating_sub(len);
                (start, total_len.saturating_sub(1))
            }
        }
    }
}

/// Parse a Range header value like "bytes=0-499" or "bytes=500-" or "bytes=-500".
fn parse_range(range_str: &str, total_len: u64) -> Option<ByteRange> {
    let range_str = range_str.trim();
    if !range_str.starts_with("bytes=") {
        return None;
    }
    let range_spec = &range_str[6..];
    let parts: Vec<&str> = range_spec.split('-').collect();
    if parts.len() != 2 {
        return None;
    }
    let start_str = parts[0].trim();
    let end_str = parts[1].trim();

    if start_str.is_empty() {
        // Suffix range: bytes=-500
        let suffix_len: u64 = end_str.parse().ok()?;
        if suffix_len == 0 || suffix_len > total_len {
            return None;
        }
        Some(ByteRange::Suffix(suffix_len))
    } else if end_str.is_empty() {
        // Open-ended range: bytes=500-
        let start: u64 = start_str.parse().ok()?;
        Some(ByteRange::StartFrom(start))
    } else {
        // Explicit range: bytes=0-499
        let start: u64 = start_str.parse().ok()?;
        let end: u64 = end_str.parse().ok()?;
        if start > end {
            return None;
        }
        Some(ByteRange::StartEnd(start, end))
    }
}

fn not_found() -> Result<Response<ResponseBody>> {
    Ok(crate::json_response(StatusCode::NOT_FOUND, r#"{"detail":"not found"}"#))
}

fn guess_content_type(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "application/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "svg" => "image/svg+xml",
        "ico" => "image/x-icon",
        "woff" => "font/woff",
        "woff2" => "font/woff2",
        "ttf" => "font/ttf",
        "txt" => "text/plain; charset=utf-8",
        "xml" => "application/xml; charset=utf-8",
        "wasm" => "application/wasm",
        "mp4" => "video/mp4",
        "webp" => "image/webp",
        _ => "application/octet-stream",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resolve_basic() {
        let dir = StaticDir::new("/tmp/static");
        assert_eq!(dir.resolve("/index.html"), Some(PathBuf::from("/tmp/static/index.html")));
        assert_eq!(dir.resolve("css/style.css"), Some(PathBuf::from("/tmp/static/css/style.css")));
    }

    #[test]
    fn test_resolve_traversal_blocked() {
        let dir = StaticDir::new("/tmp/static");
        assert_eq!(dir.resolve("/../../../etc/passwd"), None);
        assert_eq!(dir.resolve("/foo/../../bar"), None);
    }

    #[test]
    fn test_resolve_empty() {
        let dir = StaticDir::new("/tmp/static");
        assert_eq!(dir.resolve("/"), None);
        assert_eq!(dir.resolve(""), None);
    }

    #[test]
    fn test_mount_resolve_prefix() {
        let mount = StaticMount {
            prefix: "/static".to_string(),
            dir: StaticDir::new("/srv/app"),
            fallback: Some("index.html".to_string()),
        };
        // Path under prefix resolves relative to the mount root.
        assert_eq!(mount.resolve("/static/index.html"), Some(PathBuf::from("/srv/app/index.html")));
        assert_eq!(
            mount.resolve("/static/css/style.css"),
            Some(PathBuf::from("/srv/app/css/style.css"))
        );
        // Path not under prefix does not resolve.
        assert_eq!(mount.resolve("/api/users"), None);
        // Root prefix mount serves everything.
        let root = StaticMount {
            prefix: "/".to_string(),
            dir: StaticDir::new("/srv/root"),
            fallback: None,
        };
        assert_eq!(root.resolve("/favicon.ico"), Some(PathBuf::from("/srv/root/favicon.ico")));
    }

    #[test]
    fn test_mount_traversal_blocked_through_prefix() {
        let mount = StaticMount {
            prefix: "/static".to_string(),
            dir: StaticDir::new("/srv/app"),
            fallback: None,
        };
        assert_eq!(mount.resolve("/static/../../etc/passwd"), None);
    }

    #[test]
    fn test_guess_content_type() {
        assert_eq!(guess_content_type(Path::new("index.html")), "text/html; charset=utf-8");
        assert_eq!(guess_content_type(Path::new("style.css")), "text/css; charset=utf-8");
        assert_eq!(
            guess_content_type(Path::new("app.js")),
            "application/javascript; charset=utf-8"
        );
        assert_eq!(guess_content_type(Path::new("image.png")), "image/png");
        assert_eq!(guess_content_type(Path::new("font.woff2")), "font/woff2");
        assert_eq!(guess_content_type(Path::new("unknown.xyz")), "application/octet-stream");
    }

    // --- parse_range tests ---

    #[test]
    fn test_parse_range_explicit() {
        let r = parse_range("bytes=0-499", 1000).unwrap();
        assert!(matches!(r, ByteRange::StartEnd(0, 499)));
    }

    #[test]
    fn test_parse_range_open_ended() {
        let r = parse_range("bytes=500-", 1000).unwrap();
        assert!(matches!(r, ByteRange::StartFrom(500)));
    }

    #[test]
    fn test_parse_range_suffix() {
        let r = parse_range("bytes=-500", 1000).unwrap();
        assert!(matches!(r, ByteRange::Suffix(500)));
    }

    #[test]
    fn test_parse_range_single_byte() {
        let r = parse_range("bytes=0-0", 1000).unwrap();
        assert!(matches!(r, ByteRange::StartEnd(0, 0)));
    }

    #[test]
    fn test_parse_range_invalid_start_gt_end() {
        assert!(parse_range("bytes=5-3", 1000).is_none());
    }

    #[test]
    fn test_parse_range_invalid_non_bytes_prefix() {
        assert!(parse_range("bits=0-499", 1000).is_none());
    }

    #[test]
    fn test_parse_range_invalid_non_numeric() {
        assert!(parse_range("bytes=abc-def", 1000).is_none());
    }

    #[test]
    fn test_parse_range_suffix_zero() {
        assert!(parse_range("bytes=-0", 1000).is_none());
    }

    #[test]
    fn test_parse_range_suffix_exceeds_total() {
        assert!(parse_range("bytes=-9999", 100).is_none());
    }

    // --- ByteRange::into_abs_range tests ---

    #[test]
    fn test_abs_range_explicit() {
        let r = ByteRange::StartEnd(10, 20).into_abs_range(100);
        assert_eq!(r, (10, 20));
    }

    #[test]
    fn test_abs_range_explicit_clamped() {
        let r = ByteRange::StartEnd(50, 999).into_abs_range(100);
        assert_eq!(r, (50, 99));
    }

    #[test]
    fn test_abs_range_start_from() {
        let r = ByteRange::StartFrom(50).into_abs_range(100);
        assert_eq!(r, (50, 99));
    }

    #[test]
    fn test_abs_range_suffix() {
        let r = ByteRange::Suffix(30).into_abs_range(100);
        assert_eq!(r, (70, 99));
    }

    #[test]
    fn test_abs_range_suffix_equal_to_total() {
        let r = ByteRange::Suffix(100).into_abs_range(100);
        assert_eq!(r, (0, 99));
    }
}
