#![no_main]

use libfuzzer_sys::fuzz_target;
use http::{HeaderName, HeaderValue, Request, Uri, Method, Version};

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    // Try to parse as raw header name+value pairs
    if let Ok(s) = std::str::from_utf8(data) {
        for line in s.lines().take(10) {
            if let Some(pos) = line.find(':') {
                let name_str = &line[..pos].trim();
                let val_str = &line[pos + 1..].trim();
                let _ = HeaderName::from_bytes(name_str.as_bytes());
                let _ = HeaderValue::from_bytes(val_str.as_bytes());
            }
        }
    }

    // Fuzz HeaderName directly
    let _ = HeaderName::from_bytes(data);

    // Fuzz HeaderValue directly
    let _ = HeaderValue::from_bytes(data);

    // Fuzz URI parsing
    if let Ok(s) = std::str::from_utf8(data) {
        let _: Result<Uri, _> = s.parse();
    }

    // Fuzz Method parsing
    if let Ok(s) = std::str::from_utf8(data) {
        let _: Result<Method, _> = s.parse();
    }

    // Fuzz Version parsing
    let _ = match data.first().copied().unwrap_or(0) {
        0 => Version::HTTP_09,
        1 => Version::HTTP_10,
        2 => Version::HTTP_11,
        3 => Version::HTTP_2,
        4 => Version::HTTP_3,
        _ => Version::HTTP_11,
    };
});
