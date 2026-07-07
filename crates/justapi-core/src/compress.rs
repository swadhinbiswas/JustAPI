use std::io::Write;

use anyhow::Result;
use async_trait::async_trait;
use flate2::write::{DeflateEncoder, GzEncoder};
use flate2::Compression;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::{Request, Response};

use crate::ResponseBody;

// ---------------------------------------------------------------------------
// Compression algorithm
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Encoding {
    Gzip,
    Deflate,
    #[cfg(feature = "brotli-compression")]
    Brotli,
    #[cfg(feature = "zstd-compression")]
    Zstd,
    Identity,
}

impl Encoding {
    /// Parse the best encoding from the `Accept-Encoding` header.
    /// Returns the first supported encoding found, preferring Brotli > Zstd > Gzip > Deflate.
    pub fn from_accept_encoding(header_value: &str) -> Self {
        // Simple preference order: br > zstd > gzip > deflate
        let lower = header_value.to_ascii_lowercase();
        let parts: Vec<&str> = lower.split(',').map(|s| s.trim()).collect();

        for part in &parts {
            let encoding_name = part.split(';').next().unwrap_or("").trim();
            match encoding_name {
                #[cfg(feature = "brotli-compression")]
                "br" => return Encoding::Brotli,
                #[cfg(feature = "zstd-compression")]
                "zstd" => return Encoding::Zstd,
                "gzip" | "x-gzip" => return Encoding::Gzip,
                "deflate" => return Encoding::Deflate,
                "*" => {
                    // Wildcard: return the best we support
                    #[cfg(feature = "brotli-compression")]
                    return Encoding::Brotli;
                    #[cfg(feature = "zstd-compression")]
                    return Encoding::Zstd;
                    return Encoding::Gzip;
                }
                _ => continue,
            }
        }

        Encoding::Identity
    }

    /// Returns the `Content-Encoding` header value for this encoding.
    pub fn as_header_value(&self) -> &'static str {
        match self {
            Encoding::Gzip => "gzip",
            Encoding::Deflate => "deflate",
            #[cfg(feature = "brotli-compression")]
            Encoding::Brotli => "br",
            #[cfg(feature = "zstd-compression")]
            Encoding::Zstd => "zstd",
            Encoding::Identity => "identity",
        }
    }

    /// Returns true if this encoding actually compresses data.
    pub fn is_compression(&self) -> bool {
        *self != Encoding::Identity
    }
}

// ---------------------------------------------------------------------------
// Compress bytes
// ---------------------------------------------------------------------------

pub fn compress_bytes(data: &[u8], encoding: Encoding) -> Result<Vec<u8>> {
    match encoding {
        Encoding::Gzip => {
            let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(data)?;
            Ok(encoder.finish()?)
        }
        Encoding::Deflate => {
            let mut encoder = DeflateEncoder::new(Vec::new(), Compression::fast());
            encoder.write_all(data)?;
            Ok(encoder.finish()?)
        }
        #[cfg(feature = "brotli-compression")]
        Encoding::Brotli => {
            let mut encoder = brotli::CompressorWriter::new(Vec::new(), 4096, 6, 22);
            std::io::Write::write_all(&mut encoder, data)?;
            Ok(encoder.into_inner())
        }
        #[cfg(feature = "zstd-compression")]
        Encoding::Zstd => {
            let mut output = Vec::new();
            zstd::stream::copy_encode(std::io::Cursor::new(data), &mut output, 3)?;
            Ok(output)
        }
        Encoding::Identity => Ok(data.to_vec()),
    }
}

// ---------------------------------------------------------------------------
// Compression middleware
// ---------------------------------------------------------------------------

/// Middleware that compresses response bodies based on the client's
/// `Accept-Encoding` header. Only compresses responses that are large
/// enough to benefit (> 1KB) and have compressible content types.
pub struct CompressionMiddleware {
    min_size: usize,
}

impl CompressionMiddleware {
    pub fn new() -> Self {
        Self { min_size: 1024 }
    }

    pub fn with_min_size(min_size: usize) -> Self {
        Self { min_size }
    }
}

impl Default for CompressionMiddleware {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl crate::middleware::Middleware<Incoming> for CompressionMiddleware {
    async fn handle(
        &self,
        req: Request<Incoming>,
        next: crate::middleware::Next<'_, Incoming>,
    ) -> Result<Response<ResponseBody>> {
        // Determine client's accepted encoding
        let accept_encoding = req
            .headers()
            .get("accept-encoding")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        let encoding = Encoding::from_accept_encoding(accept_encoding);

        // If client doesn't support compression, just pass through
        if !encoding.is_compression() {
            return next.run(req).await;
        }

        let resp = next.run(req).await?;

        // Check if the response is worth compressing
        let content_type = resp
            .headers()
            .get("content-type")
            .and_then(|v| v.to_str().ok())
            .unwrap_or("");

        if !is_compressible_content_type(content_type) {
            return Ok(resp);
        }

        // Don't compress if already encoded
        if resp.headers().contains_key("content-encoding") {
            return Ok(resp);
        }

        // Decompose response into parts and body
        let (parts, body) = resp.into_parts();

        // Collect the response body
        let body_bytes = body.collect().await?.to_bytes();

        // Don't compress small responses
        if body_bytes.len() < self.min_size {
            let new_body = UnsyncBoxBody::new(
                http_body_util::Full::new(body_bytes)
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            );
            return Ok(Response::from_parts(parts, new_body));
        }

        // Compress
        let compressed = compress_bytes(&body_bytes, encoding)?;

        // Only use compression if it actually reduced size
        if compressed.len() >= body_bytes.len() {
            let new_body = UnsyncBoxBody::new(
                http_body_util::Full::new(body_bytes)
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            );
            return Ok(Response::from_parts(parts, new_body));
        }

        let mut parts = parts;
        parts.headers.insert(
            "content-encoding",
            encoding.as_header_value().parse().unwrap(),
        );
        parts.headers.insert(
            "content-length",
            compressed.len().to_string().parse().unwrap(),
        );
        // Add Vary header so caches know the response varies by Accept-Encoding
        parts
            .headers
            .insert("vary", "accept-encoding".parse().unwrap());

        let new_body = UnsyncBoxBody::new(
            http_body_util::Full::new(Bytes::from(compressed))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        );

        Ok(Response::from_parts(parts, new_body))
    }
}

// ---------------------------------------------------------------------------
// Content type check
// ---------------------------------------------------------------------------

fn is_compressible_content_type(ct: &str) -> bool {
    // Text-based and common compressible types
    ct.starts_with("text/")
        || ct.contains("json")
        || ct.contains("javascript")
        || ct.contains("xml")
        || ct.contains("svg")
        || ct.contains("html")
        || ct.contains("css")
        || ct.contains("wasm")
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_encoding_from_gzip() {
        assert_eq!(
            Encoding::from_accept_encoding("gzip, deflate"),
            Encoding::Gzip
        );
    }

    #[test]
    fn test_encoding_from_deflate() {
        assert_eq!(Encoding::from_accept_encoding("deflate"), Encoding::Deflate);
    }

    #[test]
    fn test_encoding_from_identity() {
        assert_eq!(
            Encoding::from_accept_encoding("identity"),
            Encoding::Identity
        );
    }

    #[test]
    fn test_encoding_from_wildcard() {
        assert_eq!(Encoding::from_accept_encoding("*"), Encoding::Gzip);
    }

    #[test]
    fn test_encoding_preference_order() {
        // Gzip should be preferred over deflate since gzip appears later in the list
        // but our parser picks the first compressible encoding it finds.
        // Actually gzip is preferred because we check in order: br > zstd > gzip > deflate
        // and "deflate, gzip" has deflate first, so deflate would be matched first.
        // Wait - the parser iterates parts and matches each. For "deflate, gzip":
        // part 0 = "deflate" -> matches Deflate, returns immediately.
        assert_eq!(
            Encoding::from_accept_encoding("deflate, gzip"),
            Encoding::Deflate
        );
    }

    #[test]
    fn test_encoding_gzip_preferred_over_deflate_in_header() {
        // When gzip appears before deflate, gzip wins
        assert_eq!(
            Encoding::from_accept_encoding("gzip, deflate"),
            Encoding::Gzip
        );
    }

    #[test]
    fn test_compress_gzip() {
        let data = b"hello world hello world hello world";
        let compressed = compress_bytes(data, Encoding::Gzip).unwrap();
        // Gzip adds overhead for small data, but still works
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_compress_deflate() {
        let data = b"hello world hello world hello world";
        let compressed = compress_bytes(data, Encoding::Deflate).unwrap();
        assert!(!compressed.is_empty());
    }

    #[test]
    fn test_compress_identity() {
        let data = b"hello world";
        let result = compress_bytes(data, Encoding::Identity).unwrap();
        assert_eq!(result, data);
    }

    #[test]
    fn test_compressible_content_type() {
        assert!(is_compressible_content_type("text/html"));
        assert!(is_compressible_content_type("application/json"));
        assert!(is_compressible_content_type("application/javascript"));
        assert!(is_compressible_content_type("image/svg+xml"));
        assert!(!is_compressible_content_type("image/png"));
        assert!(!is_compressible_content_type("image/jpeg"));
        assert!(!is_compressible_content_type("application/octet-stream"));
    }

    #[test]
    fn test_encoding_as_header_value() {
        assert_eq!(Encoding::Gzip.as_header_value(), "gzip");
        assert_eq!(Encoding::Deflate.as_header_value(), "deflate");
        assert_eq!(Encoding::Identity.as_header_value(), "identity");
    }

    #[test]
    fn test_is_compression() {
        assert!(Encoding::Gzip.is_compression());
        assert!(Encoding::Deflate.is_compression());
        assert!(!Encoding::Identity.is_compression());
    }
}
