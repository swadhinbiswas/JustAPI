//! HTTP/3 (QUIC) end-to-end integration test.
//!
//! Generates a self-signed certificate, starts an HTTP/3 server on an
//! ephemeral UDP port, and round-trips a request through the h3-quinn client.

#![cfg(feature = "http3")]

use std::net::SocketAddr;
use std::sync::Arc;

use bytes::{Buf, Bytes};
use http_body_util::Full;
use hyper::Request;
use justapi_core::http3::{serve_http3, Http3Config, Http3Handler};
use justapi_core::metrics::Metrics;
use tokio_util::sync::CancellationToken;

/// Write a self-signed cert/key pair (via rcgen) to temp files and return paths.
fn write_self_signed_cert(dir: &std::path::Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    let cert_path = dir.join("cert.pem");
    let key_path = dir.join("key.pem");
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();
    (cert_path, key_path)
}

/// Simple echo bridge handler: returns `{"echo": "<path>"}`.
fn echo_handler() -> Http3Handler {
    Arc::new(|req: Request<Full<Bytes>>| {
        Box::pin(async move {
            let path = req.uri().path().to_string();
            let body = format!(r#"{{"echo": "{}"}}"#, path).into_bytes();
            Ok::<_, anyhow::Error>((
                200,
                vec![("content-type".to_string(), "application/json".to_string())],
                body,
            ))
        })
    })
}

#[tokio::test]
async fn http3_roundtrip() {
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::DEBUG)
        .with_test_writer()
        .try_init();
    let dir = tempfile::tempdir().unwrap();
    let (cert_path, key_path) = write_self_signed_cert(dir.path());
    let cfg = Http3Config {
        cert_path: cert_path.display().to_string(),
        key_path: key_path.display().to_string(),
    };

    let cancel = CancellationToken::new();
    let addr: SocketAddr = "127.0.0.1:0".parse().unwrap();
    let bound =
        serve_http3(addr, cfg, echo_handler(), Metrics::new(), cancel.clone()).await.unwrap();

    // Trust the SAME certificate the server presents (read from the file).
    let cert_pem = std::fs::read_to_string(&cert_path).unwrap();
    let cert_der = rustls_pemfile::certs(&mut cert_pem.as_bytes()).next().unwrap().unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der).unwrap();
    let client_crypto = quinn::ClientConfig::with_root_certificates(Arc::new(roots)).unwrap();
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(client_crypto);

    let conn = endpoint.connect(bound, "localhost").unwrap().await.unwrap();
    let h3_conn = h3_quinn::Connection::new(conn);
    let (mut driver, mut send_request) = h3::client::new(h3_conn).await.unwrap();

    // Drive the client connection concurrently (required for the request
    // to complete), mirroring the h3 example's `tokio::join!` pattern.
    let drive = async move {
        Err::<(), h3::error::ConnectionError>(
            futures::future::poll_fn(|cx| driver.poll_close(cx)).await,
        )
    };

    let request = async {
        let req = Request::builder().method("GET").uri("https://localhost/hello").body(()).unwrap();
        let mut stream = send_request.send_request(req).await.unwrap();
        // Signal end of request body — otherwise the server's body read never
        // returns `None`.
        stream.finish().await.unwrap();
        let resp = stream.recv_response().await.unwrap();
        assert_eq!(resp.status(), 200);

        let mut body = Vec::new();
        while let Some(chunk) = stream.recv_data().await.unwrap() {
            body.extend_from_slice(chunk.chunk());
        }
        String::from_utf8(body).unwrap()
    };

    let (body, _drive_res) = tokio::join!(request, drive);
    assert!(body.contains(r#""echo": "/hello""#), "body was: {body}");

    cancel.cancel();
}

#[tokio::test]
#[ignore = "requires the justapi-py wheel with http3 feature in the dev venv"]
async fn http3_python_native_pipeline() {
    // Spawn a real JustAPI Python app with enable_http3 and verify a QUIC
    // request reaches the Python handler through the native pipeline.
    let dir = tempfile::tempdir().unwrap();
    let cert_path = dir.path().join("cert.pem");
    let key_path = dir.path().join("key.pem");
    let cert = rcgen::generate_simple_self_signed(vec!["localhost".to_string()]).unwrap();
    std::fs::write(&cert_path, cert.cert.pem()).unwrap();
    std::fs::write(&key_path, cert.key_pair.serialize_pem()).unwrap();

    let app_src = format!(
        r#"
from justapi import JustAPIApp
app = JustAPIApp()
app.enable_http3(cert_path={cert:?}, key_path={key:?})
@app.get("/native")
def native(request):
    return {{"handler": "python", "route": request.get("path")}}
app.run("127.0.0.1:8195")
"#,
        cert = cert_path.display().to_string(),
        key = key_path.display().to_string(),
    );
    let app_file = dir.path().join("app.py");
    std::fs::write(&app_file, app_src).unwrap();

    let repo_root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .ancestors()
        .find(|p| p.join("Cargo.toml").exists() && p.join("AGENTS.md").exists())
        .unwrap_or_else(|| std::path::Path::new(env!("CARGO_MANIFEST_DIR")).parent().unwrap())
        .to_path_buf();
    let python = std::env::var("JUSTAPI_TEST_PYTHON").unwrap_or_else(|_| {
        repo_root.join("crates/justapi-py/.venv/bin/python").display().to_string()
    });
    eprintln!("[h3-py-test] python={} repo_root={}", python, repo_root.display());
    let mut child = tokio::process::Command::new(&python)
        .arg(&app_file)
        .current_dir(repo_root)
        .kill_on_drop(true)
        .spawn()
        .expect("spawn python app");
    tokio::time::sleep(std::time::Duration::from_secs(5)).await;

    // QUIC client trusting the same cert.
    let cert_pem = std::fs::read_to_string(&cert_path).unwrap();
    let cert_der = rustls_pemfile::certs(&mut cert_pem.as_bytes()).next().unwrap().unwrap();
    let mut roots = rustls::RootCertStore::empty();
    roots.add(cert_der).unwrap();
    let client_crypto = quinn::ClientConfig::with_root_certificates(Arc::new(roots)).unwrap();
    let mut endpoint = quinn::Endpoint::client("127.0.0.1:0".parse().unwrap()).unwrap();
    endpoint.set_default_client_config(client_crypto);

    let conn = endpoint
        .connect("127.0.0.1:8195".parse().unwrap(), "localhost")
        .unwrap()
        .await
        .expect("QUIC connect to python app");
    let h3_conn = h3_quinn::Connection::new(conn);
    let (mut driver, mut send_request) = h3::client::new(h3_conn).await.unwrap();

    let drive = async move {
        Err::<(), h3::error::ConnectionError>(
            futures::future::poll_fn(|cx| driver.poll_close(cx)).await,
        )
    };
    let request = async {
        let req =
            Request::builder().method("GET").uri("https://localhost/native").body(()).unwrap();
        let mut stream = send_request.send_request(req).await.unwrap();
        stream.finish().await.unwrap();
        let resp = stream.recv_response().await.unwrap();
        assert_eq!(resp.status(), 200);
        let mut body = Vec::new();
        while let Some(chunk) = stream.recv_data().await.unwrap() {
            body.extend_from_slice(chunk.chunk());
        }
        String::from_utf8(body).unwrap()
    };
    let (body, _) = tokio::join!(request, drive);
    // The response must come from the PYTHON handler (native pipeline),
    // not a core static route.
    assert!(body.contains(r#""handler":"python""#), "body was: {body}");

    let _ = child.kill().await;
}
