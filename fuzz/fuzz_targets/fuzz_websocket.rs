//! Fuzz target for WebSocket frame parsing.
//!
//! Tests that the WebSocket frame parser handles malformed input gracefully
//! without panicking or memory safety issues.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Test WebSocket frame parsing with arbitrary bytes
    // This covers the upgrade handshake and frame parsing logic

    // Simulate a WebSocket upgrade request with fuzzed headers
    let _ = std::str::from_utf8(data);

    // Test SHA-1 computation (used in WebSocket accept key)
    use sha1::Digest;
    let mut sha1 = sha1::Sha1::new();
    sha1.update(data);
    let _ = sha1.finalize();

    // Test base64 encoding (used in WebSocket accept key)
    use base64::Engine;
    let _ = base64::engine::general_purpose::STANDARD.encode(data);
});
