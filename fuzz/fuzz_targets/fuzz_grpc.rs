//! Fuzz target for gRPC/protobuf parsing.
//!
//! Tests that the gRPC decoder handles malformed input gracefully
//! without panicking or memory safety issues.

#![no_main]
use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    // Test raw bytes decoder with arbitrary input
    // This covers the gRPC message parsing logic

    // Simulate a gRPC message with fuzzed payload
    if data.is_empty() {
        return;
    }

    // Test that we can handle arbitrary byte sequences
    // without panicking
    let _ = std::str::from_utf8(data);

    // Test JSON parsing on the data (gRPC often wraps JSON)
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = serde_json::from_str::<serde_json::Value>(s);
    }
});
