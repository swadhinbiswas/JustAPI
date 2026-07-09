#![no_main]

use libfuzzer_sys::fuzz_target;
use std::path::Path;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    if let Ok(s) = std::str::from_utf8(data) {
        // Sanitize path: prevent directory traversal
        let path = Path::new(s);
        let _ = path.components().collect::<Vec<_>>();

        // Check for path traversal patterns
        let normalized = s.replace('\\', "/");
        let has_traversal = normalized.contains("..");
        let has_null = s.contains('\0');
        let has_absolute = normalized.starts_with('/');

        // Simulate path join and canonicalization
        let base = Path::new("/var/www/static");
        let joined = base.join(path);
        let _ = joined.strip_prefix("/var/www/static");

        // Fuzz MIME type detection by extension
        if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
            let mime = match ext.to_lowercase().as_str() {
                "html" | "htm" => "text/html",
                "css" => "text/css",
                "js" => "application/javascript",
                "json" => "application/json",
                "png" => "image/png",
                "jpg" | "jpeg" => "image/jpeg",
                "gif" => "image/gif",
                "svg" => "image/svg+xml",
                "ico" => "image/x-icon",
                "woff2" => "font/woff2",
                _ => "application/octet-stream",
            };
            let _ = mime;
        }

        // Fuzz percent-decoding (as used in URL path segments)
        let mut decoded = String::with_capacity(s.len());
        let mut chars = s.chars();
        while let Some(c) = chars.next() {
            if c == '%' {
                let hex: String = chars.by_ref().take(2).collect();
                if hex.len() == 2 {
                    if let Ok(byte) = u8::from_str_radix(&hex, 16) {
                        decoded.push(byte as char);
                    }
                }
            } else {
                decoded.push(c);
            }
        }
        let _ = decoded;
    }
});
