use pyo3::prelude::*;

pub fn register(parent: &Bound<'_, PyModule>) -> PyResult<()> {
    let py = parent.py();
    let m = PyModule::new(py, "status")?;

    m.add("HTTP_100_CONTINUE", 100)?;
    m.add("HTTP_101_SWITCHING_PROTOCOLS", 101)?;
    m.add("HTTP_102_PROCESSING", 102)?;
    m.add("HTTP_103_EARLY_HINTS", 103)?;

    m.add("HTTP_200_OK", 200)?;
    m.add("HTTP_201_CREATED", 201)?;
    m.add("HTTP_202_ACCEPTED", 202)?;
    m.add("HTTP_203_NON_AUTHORITATIVE_INFORMATION", 203)?;
    m.add("HTTP_204_NO_CONTENT", 204)?;
    m.add("HTTP_205_RESET_CONTENT", 205)?;
    m.add("HTTP_206_PARTIAL_CONTENT", 206)?;
    m.add("HTTP_207_MULTI_STATUS", 207)?;
    m.add("HTTP_208_ALREADY_REPORTED", 208)?;
    m.add("HTTP_226_IM_USED", 226)?;

    m.add("HTTP_300_MULTIPLE_CHOICES", 300)?;
    m.add("HTTP_301_MOVED_PERMANENTLY", 301)?;
    m.add("HTTP_302_FOUND", 302)?;
    m.add("HTTP_303_SEE_OTHER", 303)?;
    m.add("HTTP_304_NOT_MODIFIED", 304)?;
    m.add("HTTP_305_USE_PROXY", 305)?;
    m.add("HTTP_306_RESERVED", 306)?;
    m.add("HTTP_307_TEMPORARY_REDIRECT", 307)?;
    m.add("HTTP_308_PERMANENT_REDIRECT", 308)?;

    m.add("HTTP_400_BAD_REQUEST", 400)?;
    m.add("HTTP_401_UNAUTHORIZED", 401)?;
    m.add("HTTP_402_PAYMENT_REQUIRED", 402)?;
    m.add("HTTP_403_FORBIDDEN", 403)?;
    m.add("HTTP_404_NOT_FOUND", 404)?;
    m.add("HTTP_405_METHOD_NOT_ALLOWED", 405)?;
    m.add("HTTP_406_NOT_ACCEPTABLE", 406)?;
    m.add("HTTP_407_PROXY_AUTHENTICATION_REQUIRED", 407)?;
    m.add("HTTP_408_REQUEST_TIMEOUT", 408)?;
    m.add("HTTP_409_CONFLICT", 409)?;
    m.add("HTTP_410_GONE", 410)?;
    m.add("HTTP_411_LENGTH_REQUIRED", 411)?;
    m.add("HTTP_412_PRECONDITION_FAILED", 412)?;
    m.add("HTTP_413_CONTENT_TOO_LARGE", 413)?;
    m.add("HTTP_414_URI_TOO_LONG", 414)?;
    m.add("HTTP_415_UNSUPPORTED_MEDIA_TYPE", 415)?;
    m.add("HTTP_416_RANGE_NOT_SATISFIABLE", 416)?;
    m.add("HTTP_417_EXPECTATION_FAILED", 417)?;
    m.add("HTTP_418_IM_A_TEAPOT", 418)?;
    m.add("HTTP_421_MISDIRECTED_REQUEST", 421)?;
    m.add("HTTP_422_UNPROCESSABLE_CONTENT", 422)?;
    m.add("HTTP_423_LOCKED", 423)?;
    m.add("HTTP_424_FAILED_DEPENDENCY", 424)?;
    m.add("HTTP_425_TOO_EARLY", 425)?;
    m.add("HTTP_426_UPGRADE_REQUIRED", 426)?;
    m.add("HTTP_428_PRECONDITION_REQUIRED", 428)?;
    m.add("HTTP_429_TOO_MANY_REQUESTS", 429)?;
    m.add("HTTP_431_REQUEST_HEADER_FIELDS_TOO_LARGE", 431)?;
    m.add("HTTP_451_UNAVAILABLE_FOR_LEGAL_REASONS", 451)?;

    m.add("HTTP_500_INTERNAL_SERVER_ERROR", 500)?;
    m.add("HTTP_501_NOT_IMPLEMENTED", 501)?;
    m.add("HTTP_502_BAD_GATEWAY", 502)?;
    m.add("HTTP_503_SERVICE_UNAVAILABLE", 503)?;
    m.add("HTTP_504_GATEWAY_TIMEOUT", 504)?;
    m.add("HTTP_505_HTTP_VERSION_NOT_SUPPORTED", 505)?;
    m.add("HTTP_506_VARIANT_ALSO_NEGOTIATES", 506)?;
    m.add("HTTP_507_INSUFFICIENT_STORAGE", 507)?;
    m.add("HTTP_508_LOOP_DETECTED", 508)?;
    m.add("HTTP_510_NOT_EXTENDED", 510)?;
    m.add("HTTP_511_NETWORK_AUTHENTICATION_REQUIRED", 511)?;

    m.add("WS_1000_NORMAL_CLOSURE", 1000)?;
    m.add("WS_1001_GOING_AWAY", 1001)?;
    m.add("WS_1002_PROTOCOL_ERROR", 1002)?;
    m.add("WS_1003_UNSUPPORTED_DATA", 1003)?;
    m.add("WS_1005_NO_STATUS_RCVD", 1005)?;
    m.add("WS_1006_ABNORMAL_CLOSURE", 1006)?;
    m.add("WS_1007_INVALID_FRAME_PAYLOAD_DATA", 1007)?;
    m.add("WS_1008_POLICY_VIOLATION", 1008)?;
    m.add("WS_1009_MESSAGE_TOO_BIG", 1009)?;
    m.add("WS_1010_MANDATORY_EXT", 1010)?;
    m.add("WS_1011_INTERNAL_ERROR", 1011)?;
    m.add("WS_1012_SERVICE_RESTART", 1012)?;
    m.add("WS_1013_TRY_AGAIN_LATER", 1013)?;
    m.add("WS_1014_BAD_GATEWAY", 1014)?;
    m.add("WS_1015_TLS_HANDSHAKE", 1015)?;

    parent.add_submodule(&m)?;

    let parent_name: String = parent.getattr("__name__")?.extract()?;
    let full_name = format!("{}.status", parent_name);
    let sys = py.import("sys")?;
    let modules = sys.getattr("modules")?;
    modules.set_item(full_name, &m)?;
    Ok(())
}
