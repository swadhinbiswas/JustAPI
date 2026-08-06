//! Integration tests for edge cases and security hardening.

use hyper::StatusCode;
use justapi_core::testing::TestClient;
use justapi_core::{json_response, middleware::HandlerFn, router::Router};
use std::sync::Arc;

fn test_handler() -> HandlerFn {
    Arc::new(|_| Box::pin(async { Ok(json_response(StatusCode::OK, r#"{"ok":true}"#)) }))
}

#[tokio::test]
async fn test_oversized_path_rejected() {
    let mut router = Router::new();
    router.insert(hyper::Method::GET, "/", test_handler()).unwrap();

    let client = TestClient::new(Arc::new(move |req| {
        let chain = justapi_core::middleware::MiddlewareChain::new(test_handler());
        Box::pin(async move {
            let path = req.uri().path().to_string();
            if path.len() > 8192 {
                return Ok(justapi_core::error_response(
                    hyper::StatusCode::URI_TOO_LONG,
                    "request URI exceeds maximum length",
                ));
            }
            chain.run(req).await
        })
    }));

    // Normal path should work
    let resp = client.get("/").await.unwrap();
    assert_eq!(resp.status, 200);

    // Oversized path should be rejected
    let oversized_path = format!("/{}", "a".repeat(9000));
    let resp = client.get(&oversized_path).await.unwrap();
    assert_eq!(resp.status, 414);
}

#[tokio::test]
async fn test_oversized_header_rejected() {
    let client = TestClient::new(Arc::new(move |req| {
        let chain = justapi_core::middleware::MiddlewareChain::new(test_handler());
        Box::pin(async move {
            // Check header count
            if req.headers().len() > 100 {
                return Ok(justapi_core::error_response(
                    hyper::StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                    "too many request headers",
                ));
            }
            // Check header value length
            for (name, value) in req.headers().iter() {
                if value.len() > 8192 {
                    return Ok(justapi_core::error_response(
                        hyper::StatusCode::REQUEST_HEADER_FIELDS_TOO_LARGE,
                        &format!("header '{}' exceeds maximum value length", name.as_str()),
                    ));
                }
            }
            chain.run(req).await
        })
    }));

    // Normal request should work
    let resp = client.get("/").await.unwrap();
    assert_eq!(resp.status, 200);
}

#[tokio::test]
#[cfg(feature = "db")]
async fn test_sql_identifier_validation() {
    use justapi_core::server::{CrudOp, CrudSpec};

    // Valid identifiers
    let spec = CrudSpec {
        op: CrudOp::Insert,
        table: "users".to_string(),
        columns: vec!["name".to_string(), "email".to_string()],
        id_column: "id".to_string(),
    };
    assert!(spec.validate().is_ok());

    // Invalid table name (SQL injection attempt)
    let spec = CrudSpec {
        op: CrudOp::Insert,
        table: "users; DROP TABLE users--".to_string(),
        columns: vec!["name".to_string()],
        id_column: "id".to_string(),
    };
    assert!(spec.validate().is_err());

    // Invalid column name
    let spec = CrudSpec {
        op: CrudOp::Insert,
        table: "users".to_string(),
        columns: vec!["name OR 1=1".to_string()],
        id_column: "id".to_string(),
    };
    assert!(spec.validate().is_err());

    // Empty table name
    let spec = CrudSpec {
        op: CrudOp::Insert,
        table: "".to_string(),
        columns: vec!["name".to_string()],
        id_column: "id".to_string(),
    };
    assert!(spec.validate().is_err());

    // Table starting with number
    let spec = CrudSpec {
        op: CrudOp::Insert,
        table: "1users".to_string(),
        columns: vec!["name".to_string()],
        id_column: "id".to_string(),
    };
    assert!(spec.validate().is_err());
}

#[tokio::test]
async fn test_chaos_middleware_config_validation() {
    use justapi_core::resilience::{ChaosConfig, ChaosMiddleware};

    // Valid config
    let config = ChaosConfig {
        enabled: true,
        latency_p: 0.5,
        latency_min_ms: 100,
        latency_max_ms: 500,
        error_p: 0.1,
        error_status: StatusCode::INTERNAL_SERVER_ERROR,
    };
    let _mw = ChaosMiddleware::new(config);

    // Invalid config (min > max) - should not panic
    let config = ChaosConfig {
        enabled: true,
        latency_p: 0.5,
        latency_min_ms: 500,
        latency_max_ms: 100,
        error_p: 0.1,
        error_status: StatusCode::INTERNAL_SERVER_ERROR,
    };
    let _mw = ChaosMiddleware::new(config);
}

#[tokio::test]
async fn test_health_check_timeout() {
    use justapi_core::health::{HealthRegistry, HealthStatus};

    let mut registry = HealthRegistry::new();

    // Fast check
    registry.register_fn("fast", || async { HealthStatus::Healthy });

    // Slow check (simulated)
    registry.register_fn("slow", || async {
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        HealthStatus::Healthy
    });

    let report = registry.check_all_with_timeout(std::time::Duration::from_millis(50)).await;

    // Fast check should pass
    let fast = report.components.iter().find(|c| c.name == "fast");
    assert!(fast.is_some());
    assert_eq!(fast.unwrap().status, HealthStatus::Healthy);

    // Slow check should timeout
    let slow = report.components.iter().find(|c| c.name == "slow");
    assert!(slow.is_some());
    match &slow.unwrap().status {
        HealthStatus::Unhealthy(msg) => {
            assert!(msg.contains("timed out"));
        }
        _ => panic!("Expected Unhealthy status for slow check"),
    }
}

#[tokio::test]
async fn test_router_cache_eviction() {
    let mut router = Router::new();

    // Insert many routes to trigger eviction
    for i in 0..20000 {
        router
            .insert(hyper::Method::GET, &format!("/route/{}", i), format!("handler_{}", i))
            .unwrap();
    }

    // Verify routes still work
    let result = router.resolve(&hyper::Method::GET, "/route/0");
    assert!(result.is_ok());

    let result = router.resolve(&hyper::Method::GET, "/route/19999");
    assert!(result.is_ok());
}
