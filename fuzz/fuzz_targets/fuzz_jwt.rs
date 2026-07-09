#![no_main]

use libfuzzer_sys::fuzz_target;
use jsonwebtoken::{decode, decode_header, Validation, DecodingKey, Algorithm};

fuzz_target!(|data: &[u8]| {
    if data.len() < 4 {
        return;
    }
    let key = DecodingKey::from_secret(b"test-secret-key-for-fuzzing");
    let validation = Validation::new(Algorithm::HS256);

    if let Ok(s) = std::str::from_utf8(data) {
        let _ = decode_header(s);
        let _ = decode::<serde_json::Value>(s, &key, &validation);
    }

    let alt_key = DecodingKey::from_secret(data);
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = decode::<serde_json::Value>(s, &alt_key, &validation);
    }
});
