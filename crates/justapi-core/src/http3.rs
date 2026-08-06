//! HTTP/3 (QUIC) server support.
//!
//! HTTP/3 runs over UDP via QUIC (RFC 9000) with TLS 1.3 built in (RFC 9001),
//! multiplexing requests on a single connection. It is a *separate* transport
//! from the TCP-based HTTP/1.1/HTTP/2 path: the listener binds a UDP socket,
//! quinn provides the QUIC transport, and `h3` provides the HTTP/3 protocol
//! framing on top.
//!
//! Integration model: the HTTP/3 endpoint serves the *same* application
//! handler the TCP path uses. Because quinn/h3 produce their own request
//! types, requests are bridged into a `hyper::Request<Full<Bytes>>` and run
//! through a caller-provided generic handler closure (see
//! [`serve_http3`]). This keeps one application pipeline for both transports.
//!
//! HTTPS/QUIC requires a TLS certificate (self-signed is fine for testing).
//! Enable with the `http3` feature flag.

use std::net::SocketAddr;
use std::sync::Arc;

use anyhow::Result;
use bytes::{Buf, Bytes};
use http_body_util::Full;
use hyper::Request;

use crate::metrics::Metrics;
use crate::middleware::MiddlewareChain;

/// QUIC server endpoint configuration (certificates are required for TLS).
#[derive(Clone, Debug)]
pub struct Http3Config {
    /// TLS certificate chain (PEM).
    pub cert_path: String,
    /// TLS private key (PEM).
    pub key_path: String,
}

/// Bridge handler used by the HTTP/3 endpoint. It maps a bridged
/// `hyper::Request<Full<Bytes>>` to the response (status, headers, body).
pub type Http3Handler = Arc<
    dyn Fn(
            Request<Full<Bytes>>,
        ) -> std::pin::Pin<
            Box<
                dyn std::future::Future<Output = Result<(u16, Vec<(String, String)>, Vec<u8>)>>
                    + Send,
            >,
        > + Send
        + Sync,
>;

/// Build a QUIC server config from PEM cert/key files (reuses rustls).
fn quic_server_config(cfg: &Http3Config) -> Result<quinn::ServerConfig> {
    let certs = {
        let data = std::fs::read(&cfg.cert_path)?;
        let mut reader = std::io::BufReader::new(data.as_slice());
        rustls_pemfile::certs(&mut reader).collect::<Result<Vec<_>, _>>()?
    };
    let key = {
        let data = std::fs::read(&cfg.key_path)?;
        let mut reader = std::io::BufReader::new(data.as_slice());
        rustls_pemfile::private_key(&mut reader)?
            .ok_or_else(|| anyhow::anyhow!("no private key found in {}", cfg.key_path))?
    };
    let mut config = quinn::ServerConfig::with_single_cert(certs, key)?;
    config.transport_config(Arc::new(quinn::TransportConfig::default()));
    Ok(config)
}

/// Run an HTTP/3 (QUIC) server on a UDP socket bound to `addr`.
///
/// `handler` is the application bridge handler; `metrics` records request
/// status/latency for the HTTP/3 path. The accept loop runs in the background
/// until `cancel` is triggered or the socket errors. Returns the bound
/// address.
pub async fn serve_http3(
    addr: SocketAddr,
    cfg: Http3Config,
    handler: Http3Handler,
    metrics: Metrics,
    cancel: tokio_util::sync::CancellationToken,
) -> Result<SocketAddr> {
    let server_config = quic_server_config(&cfg)?;
    let std_socket = std::net::UdpSocket::bind(addr)?;
    let local_addr = std_socket.local_addr()?;

    let endpoint = quinn::Endpoint::new(
        quinn::EndpointConfig::default(),
        Some(server_config),
        std_socket,
        Arc::new(quinn::TokioRuntime),
    )?;

    tracing::info!("HTTP/3 listening on udp://{} (QUIC, ALPN h3)", local_addr);

    tokio::spawn(async move {
        loop {
            tokio::select! {
                _ = cancel.cancelled() => {
                    tracing::info!("HTTP/3 shutdown signal received, stopping accept loop");
                    break;
                }
                incoming = endpoint.accept() => {
                    match incoming {
                        Some(incoming) => {
                            match incoming.accept() {
                                Ok(connecting) => {
                                    let handler = handler.clone();
                                    let metrics = metrics.clone();
                                    tokio::spawn(async move {
                                        match connecting.await {
                                            Ok(conn) => {
                                                tracing::debug!("HTTP/3 connection established");
                                                if let Err(e) = serve_quic_conn(conn, handler, metrics).await {
                                                    tracing::debug!("HTTP/3 connection error: {}", e);
                                                }
                                            }
                                            Err(e) => tracing::debug!("HTTP/3 handshake rejected: {}", e),
                                        }
                                    });
                                }
                                Err(e) => tracing::debug!("HTTP/3 accept error: {}", e),
                            }
                        }
                        None => {
                            tracing::warn!("HTTP/3 accept returned None (endpoint closed)");
                            break;
                        }
                    }
                }
            }
        }
    });

    Ok(local_addr)
}

async fn serve_quic_conn(
    conn: quinn::Connection,
    handler: Http3Handler,
    metrics: Metrics,
) -> Result<()> {
    let conn = h3_quinn::Connection::new(conn);
    let mut h3_conn = h3::server::builder().build(conn).await?;
    tracing::debug!("HTTP/3 connection built, waiting for requests");

    loop {
        match h3_conn.accept().await {
            Ok(Some(resolver)) => {
                let handler = handler.clone();
                let metrics = metrics.clone();
                tokio::spawn(async move {
                    let start = std::time::Instant::now();
                    match resolver.resolve_request().await {
                        Ok((req, mut stream)) => {
                            let body = match collect_body(&mut stream).await {
                                Ok(b) => b,
                                Err(_) => {
                                    metrics.record_status(hyper::StatusCode::BAD_REQUEST);
                                    metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);
                                    return;
                                }
                            };
                            let bridged = bridge_request(req, body);
                            match handler(bridged).await {
                                Ok((status, headers, resp_body)) => {
                                    let mut resp = http::Response::builder()
                                        .status(
                                            http::StatusCode::from_u16(status)
                                                .unwrap_or(http::StatusCode::INTERNAL_SERVER_ERROR),
                                        )
                                        .body(())
                                        .expect("valid response");
                                    for (k, v) in headers {
                                        if let (Ok(name), Ok(value)) = (
                                            http::header::HeaderName::from_bytes(k.as_bytes()),
                                            http::HeaderValue::from_str(&v),
                                        ) {
                                            resp.headers_mut().insert(name, value);
                                        }
                                    }
                                    if stream.send_response(resp).await.is_err() {
                                        return;
                                    }
                                    if !resp_body.is_empty()
                                        && stream.send_data(Bytes::from(resp_body)).await.is_err()
                                    {
                                        return;
                                    }
                                    let _ = stream.finish().await;
                                    metrics.record_status(
                                        hyper::StatusCode::from_u16(status)
                                            .unwrap_or(hyper::StatusCode::INTERNAL_SERVER_ERROR),
                                    );
                                    metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);
                                }
                                Err(e) => {
                                    tracing::error!("HTTP/3 handler error: {:#}", e);
                                    metrics.record_status(hyper::StatusCode::INTERNAL_SERVER_ERROR);
                                    metrics.record_latency(start.elapsed().as_secs_f64() * 1000.0);
                                }
                            }
                        }
                        Err(e) => {
                            tracing::debug!("HTTP/3 request resolution error: {}", e);
                        }
                    }
                });
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }
    Ok(())
}

async fn collect_body<S>(
    stream: &mut h3::server::RequestStream<S, Bytes>,
) -> Result<Vec<u8>, String>
where
    S: h3::quic::BidiStream<Bytes> + Send + 'static,
{
    let mut buf = Vec::new();
    while let Some(chunk) = stream.recv_data().await.map_err(|e| e.to_string())? {
        buf.extend_from_slice(chunk.chunk());
        if buf.len() > 50 * 1024 * 1024 {
            return Err("body exceeds maximum size".to_string());
        }
    }
    Ok(buf)
}

fn bridge_request(req: http::Request<()>, body: Vec<u8>) -> Request<Full<Bytes>> {
    let mut builder = Request::builder().method(req.method().clone()).uri(req.uri().clone());
    for (k, v) in req.headers() {
        builder = builder.header(k.clone(), v.clone());
    }
    builder.body(Full::new(Bytes::from(body))).expect("valid bridged request")
}

/// Convenience: build a bridge handler from a `MiddlewareChain<Full<Bytes>>`.
pub fn chain_to_http3_handler(chain: MiddlewareChain<Full<Bytes>>) -> Http3Handler {
    Arc::new(move |req: Request<Full<Bytes>>| {
        let chain = chain.clone();
        Box::pin(async move {
            let resp = chain.run(req).await?;
            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .map(|(k, v)| (k.as_str().to_string(), v.to_str().unwrap_or("").to_string()))
                .collect();
            let resp_body = http_body_util::BodyExt::collect(resp.into_body())
                .await
                .map(|c| c.to_bytes().to_vec())?;
            Ok((status, headers, resp_body))
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn quic_server_config_rejects_missing_key() {
        let cfg = Http3Config {
            cert_path: "/nonexistent/cert.pem".into(),
            key_path: "/nonexistent/key.pem".into(),
        };
        assert!(quic_server_config(&cfg).is_err());
    }
}
