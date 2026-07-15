#![no_main]

use bytes::Bytes;
use http_body_util::Full;
use justapi_core::multipart::parse_multipart;
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    let boundary = "JUSTAPIFUZZBOUNDARY";
    let ct = format!("multipart/form-data; boundary={}", boundary);

    // Feed attacker-controlled bytes through the multipart parser. The parser
    // handles untrusted input, so it must never panic — only return Ok/Err.
    // Wrapping raw fuzz bytes as a body means even malformed structures are
    // exercised (missing boundaries, embedded NULs, huge fields, etc.).
    let rt = match tokio::runtime::Runtime::new() {
        Ok(rt) => rt,
        Err(_) => return,
    };
    rt.block_on(async {
        let body = Full::new(Bytes::from(data.to_vec()));
        let _ = parse_multipart(body, &ct).await;
    });
});
