---
title: HTTP/3 (QUIC)
description: Serve JustAPI over HTTP/3 (QUIC) — the one transport no other Python web framework ships.
keywords: [JustAPI, HTTP/3, QUIC, UDP, TLS 1.3, fastapi alternative]
---

## What HTTP/3 gives you

HTTP/3 runs over **UDP via QUIC** (RFC 9000) with TLS 1.3 built in. Unlike
HTTP/1.1 and HTTP/2 (both TCP), QUIC multiplexes many requests on a single
connection with **no head-of-line blocking**, supports **0-RTT resumption**,
and handles connection migration natively. JustAPI is the only Python web
framework with an HTTP/3 server.

## Enabling HTTP/3

```python
from justapi import JustAPIApp

app = JustAPIApp()
app.enable_http3(
    cert_path="certs/fullchain.pem",
    key_path="certs/privkey.pem",
)

app.get("/", lambda r: {"message": "hello"})
app.run("0.0.0.0:443")
```

That's it — `run(addr)` binds the **same address** over UDP and serves the
same application handler over HTTP/3 **and** HTTP/1.1/2 over TCP. Routes,
dependencies, schema validation, sessions, and the GIL-pool dispatch all work
identically over QUIC (verified end-to-end in the test suite).

Requirements:

- PEM **certificate** and **private key** files (QUIC mandates TLS 1.3 —
  a self-signed cert works for testing, use a real CA for production).
- A build with the `http3` feature enabled. The default wheel does **not**
  include it; rebuild with:
  ```bash
  maturin develop --features http3   # dev
  maturin build --release --features http3   # wheel
  ```

Without the feature, `app.enable_http3(...)` raises `NotImplementedError`.

## Testing with a QUIC client

```bash
# curl with HTTP/3 support (curl >= 7.66 built with --http3)
curl --http3 https://127.0.0.1:443/

# Python (h3 / aioquic)
pip install aioquic
```

## Notes

- HTTP/3 and the TCP listeners share the same port (UDP vs TCP — no conflict).
- Graceful shutdown (SIGTERM/Ctrl+C) stops the QUIC listener too.
- Metrics record HTTP/3 requests through the same pipeline as TCP.
