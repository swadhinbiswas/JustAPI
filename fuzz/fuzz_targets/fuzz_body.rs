#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.len() < 2 {
        return;
    }

    // Fuzz JSON body parsing (used by request validation)
    let _: Result<serde_json::Value, _> = serde_json::from_slice(data);

    // Fuzz JSON Schema validation
    let schema = r#"{"type":"object","properties":{"name":{"type":"string"},"age":{"type":"integer","minimum":0}},"required":["name"]}"#;
    let _ = justapi_core::validate::validate_json_schema(data, schema);

    // Fuzz with recursive schema
    let recursive_schema = r##"{"type":"object","additionalProperties":{"$ref":"#"}}"##;
    let _ = justapi_core::validate::validate_json_schema(data, recursive_schema);

    // Fuzz deeply nested JSON parsing (stack depth)
    if data.len() > 10 {
        let _: Result<serde_json::Value, _> = serde_json::from_slice(data);
    }
});
