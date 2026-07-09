#![no_main]

use libfuzzer_sys::fuzz_target;

fuzz_target!(|data: &[u8]| {
    if data.is_empty() {
        return;
    }

    if let Ok(s) = std::str::from_utf8(data) {
        // Build a router with sample routes and fuzz the path matching
        use http::Method;
        let mut router = justapi_core::router::Router::new();
        let _ = router.insert(Method::GET, "/users/{id}", 1usize);
        let _ = router.insert(Method::GET, "/users/{id}/posts", 2);
        let _ = router.insert(Method::GET, "/users/{id}/posts/{post_id}", 3);
        let _ = router.insert(Method::GET, "/static/{*path}", 4);
        let _ = router.insert(Method::GET, "/api/v1/{resource}", 5);
        let _ = router.insert(Method::GET, "/api/v1/{resource}/{action}", 6);
        let _ = router.insert(Method::GET, "/health", 7);
        let _ = router.insert(Method::GET, "/live", 8);
        let _ = router.insert(Method::GET, "/ready", 9);

        let _ = router.at(&Method::GET, s);
    }
});
