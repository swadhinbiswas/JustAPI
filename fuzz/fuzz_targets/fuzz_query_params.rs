#![no_main]

use libfuzzer_sys::fuzz_target;
use std::collections::HashMap;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    if let Ok(s) = std::str::from_utf8(data) {
        // Fuzz URL query string parsing
        let _: HashMap<String, String> = serde_urlencoded::from_str(s).unwrap_or_default();

        // Fuzz full URL parsing with query parameters
        if s.len() > 4 && s.contains('=') {
            let url = format!("http://localhost:8080/test?{}", s);
            let _: Result<http::Uri, _> = url.parse();
        }
    }
});
