#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    // 1. Fuzz JSON Schema validation with arbitrary input bytes
    let schema = r#"{
        "type": "object",
        "properties": {
            "name": {"type": "string", "minLength": 1, "maxLength": 100},
            "age": {"type": "integer", "minimum": 0, "maximum": 150},
            "email": {"type": "string", "format": "email"},
            "website": {"type": "string", "format": "uri"},
            "id": {"type": "string", "format": "uuid"},
            "tags": {"type": "array", "items": {"type": "string"}}
        },
        "required": ["name"]
    }"#;

    let _ = justapi_core::validate::validate_json_schema(data, schema);

    // 2. Fuzz precompiled validator execution
    if let Ok(compiled) = justapi_core::validate::compile_schema(schema) {
        let _ = compiled.validate(data);
    }

    // 3. Fuzz query string parsing
    if let Ok(s) = std::str::from_utf8(data) {
        let _ = justapi_core::validate::parse_query::<serde_json::Value>(s);
    }

    // 4. Fuzz route matching with dynamic paths
    use http::Method;
    let mut router = justapi_core::router::Router::<usize>::new();
    let _ = router.insert(Method::GET, "/users/{id}", 1);
    let _ = router.insert(Method::POST, "/api/v1/{*rest}", 2);

    if let Ok(path) = std::str::from_utf8(data) {
        let _ = router.at(&Method::GET, path);
        let _ = router.at(&Method::POST, path);
    }
});
