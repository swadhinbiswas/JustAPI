//! Testing utilities — `TestClient` for in-process request/response.
//!
//! Uses a tokio duplex pipe to send raw HTTP bytes through the hyper HTTP/1.1
//! parser, producing real `Incoming` bodies without a TCP socket.

use hyper::{Method, Request};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

use hyper_util::rt::TokioIo;

use crate::middleware::HandlerFn;
use tracing;

/// An in-memory HTTP test response.
#[derive(Debug, Clone)]
pub struct TestResponse {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    pub body: Vec<u8>,
}

/// A test client that sends requests through the full JustAPI pipeline
/// without opening a TCP socket.
#[derive(Clone)]
pub struct TestClient {
    handler: HandlerFn,
}

impl TestClient {
    pub fn new(handler: HandlerFn) -> Self {
        Self { handler }
    }

    pub async fn get(&self, path: &str) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::GET, path, Vec::new(), &[]).await
    }

    /// GET with extra request headers (e.g. to exercise header-aware routing
    /// or coalescing keys).
    pub async fn get_with(
        &self,
        path: &str,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::GET, path, Vec::new(), headers).await
    }

    pub async fn post(&self, path: &str, body: Vec<u8>) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::POST, path, body, &[]).await
    }

    /// POST with extra request headers (e.g. a `Content-Type` carrying the
    /// `multipart/form-data` boundary for file-upload tests).
    pub async fn post_with(
        &self,
        path: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::POST, path, body, headers).await
    }

    pub async fn put(&self, path: &str, body: Vec<u8>) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::PUT, path, body, &[]).await
    }

    pub async fn patch(&self, path: &str, body: Vec<u8>) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::PATCH, path, body, &[]).await
    }

    pub async fn delete(&self, path: &str) -> Result<TestResponse, anyhow::Error> {
        self.request(Method::DELETE, path, Vec::new(), &[]).await
    }

    async fn request(
        &self,
        method: Method,
        path: &str,
        body: Vec<u8>,
        headers: &[(&str, &str)],
    ) -> Result<TestResponse, anyhow::Error> {
        let handler = self.handler.clone();
        let (mut client, server) = tokio::io::duplex(65536);

        // Spawn the server side: feed the handler through hyper HTTP/1.1
        tokio::spawn(async move {
            let svc = hyper::service::service_fn(move |req: Request<hyper::body::Incoming>| {
                let handler = handler.clone();
                async move { handler(req).await }
            });
            let io = TokioIo::new(server);
            match hyper::server::conn::http1::Builder::new().serve_connection(io, svc).await {
                Ok(()) => {}
                Err(e) => {
                    // A body-stream error (e.g. a validated stream aborting on an
                    // invalid item) surfaces here as a connection-level error. That
                    // is expected application behavior, not a harness bug, so log it
                    // instead of panicking — panicking aborts the whole test process
                    // under `panic = "abort"`.
                    tracing::debug!("Test server connection closed with error: {}", e);
                }
            }
        });

        // Build and send the raw HTTP request with Connection: close
        let method_str = method.to_string();
        let body_len = body.len();
        let mut request_bytes =
            format!("{} {} HTTP/1.1\r\nHost: test\r\nConnection: close\r\n", method_str, path,)
                .into_bytes();
        for (name, value) in headers {
            request_bytes.extend_from_slice(format!("{}: {}\r\n", name, value).as_bytes());
        }
        request_bytes.extend_from_slice(format!("Content-Length: {}\r\n\r\n", body_len).as_bytes());
        if body_len > 0 {
            request_bytes.extend_from_slice(&body);
        }

        client.write_all(&request_bytes).await?;

        // Wait for the server to process the request and write the response.
        // The server end of the duplex is dropped when serve_connection returns,
        // which signals EOF on our read half.
        let mut buf = Vec::new();
        client.read_to_end(&mut buf).await?;

        // Parse the raw HTTP response
        parse_response(&buf)
    }
}

fn parse_response(raw: &[u8]) -> Result<TestResponse, anyhow::Error> {
    let header_end = find_double_crlf(raw)
        .ok_or_else(|| anyhow::anyhow!("No header/body separator found in response"))?;

    let header_section = &raw[..header_end];
    let body = &raw[header_end + 4..];

    let header_str = std::str::from_utf8(header_section)
        .map_err(|e| anyhow::anyhow!("Invalid UTF-8 in response headers: {}", e))?;

    let mut lines = header_str.lines();

    // Parse status line: "HTTP/1.1 200 OK"
    let status_line = lines.next().unwrap_or("");
    let status: u16 = status_line.split_whitespace().nth(1).unwrap_or("500").parse().unwrap_or(500);

    // Parse headers
    let headers: Vec<(String, String)> = lines
        .filter(|l| !l.is_empty())
        .filter_map(|l| {
            let mut parts = l.splitn(2, ": ");
            let name = parts.next()?.to_string();
            let value = parts.next().unwrap_or("").to_string();
            Some((name, value))
        })
        .collect();

    let is_chunked = headers.iter().any(|(k, v)| {
        k.eq_ignore_ascii_case("transfer-encoding") && v.eq_ignore_ascii_case("chunked")
    });

    let body = if is_chunked { decode_chunked(body)? } else { body.to_vec() };

    Ok(TestResponse { status, headers, body })
}

/// Decode RFC 9112 chunked-transfer-encoding framing into the raw body bytes.
fn decode_chunked(body: &[u8]) -> Result<Vec<u8>, anyhow::Error> {
    let mut out = Vec::new();
    let mut pos = 0;
    while pos < body.len() {
        // Find the end of the chunk-size line (terminated by \r\n or \n).
        let line_end = body[pos..]
            .iter()
            .position(|&b| b == b'\n')
            .map(|i| pos + i)
            .ok_or_else(|| anyhow::anyhow!("Truncated chunked body"))?;
        let size_line = &body[pos..line_end];
        // Strip a trailing \r if present.
        let size_line = if size_line.last() == Some(&b'\r') {
            &size_line[..size_line.len() - 1]
        } else {
            size_line
        };
        let size_str = std::str::from_utf8(size_line)
            .map_err(|e| anyhow::anyhow!("Invalid chunk size: {}", e))?;
        // Allow optional chunk-ext after a ';'.
        let size_hex = size_str.split(';').next().unwrap_or("").trim();
        let size: usize = usize::from_str_radix(size_hex, 16)
            .map_err(|e| anyhow::anyhow!("Invalid chunk size: {}", e))?;
        if size == 0 {
            break;
        }
        let data_start = line_end + 1;
        let data_end = data_start + size;
        if data_end > body.len() {
            return Err(anyhow::anyhow!("Chunk claims more data than available"));
        }
        out.extend_from_slice(&body[data_start..data_end]);
        // Skip the chunk-data trailing CRLF.
        pos = data_end + if body[data_end..].starts_with(b"\r\n") { 2 } else { 1 };
    }
    Ok(out)
}

fn find_double_crlf(data: &[u8]) -> Option<usize> {
    data.windows(4).position(|w| w == b"\r\n\r\n")
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_response;
    use crate::middleware::HandlerFn;
    use hyper::StatusCode;

    fn ok_handler() -> HandlerFn {
        std::sync::Arc::new(|_req| {
            Box::pin(async move { Ok(json_response(StatusCode::OK, r#"{"status":"ok"}"#)) })
        })
    }

    #[tokio::test]
    async fn test_test_client_get() {
        let client = TestClient::new(ok_handler());
        let resp = client.get("/test").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, br#"{"status":"ok"}"#);
    }

    #[tokio::test]
    async fn test_test_client_post() {
        let client = TestClient::new(ok_handler());
        let resp = client.post("/test", b"{}".to_vec()).await.unwrap();
        assert_eq!(resp.status, 200);
    }

    #[tokio::test]
    async fn test_test_client_404() {
        let handler: HandlerFn = std::sync::Arc::new(|_req| {
            Box::pin(
                async move { Ok(json_response(StatusCode::NOT_FOUND, r#"{"error":"not found"}"#)) },
            )
        });
        let client = TestClient::new(handler);
        let resp = client.get("/nonexistent").await.unwrap();
        assert_eq!(resp.status, 404);
    }
}
