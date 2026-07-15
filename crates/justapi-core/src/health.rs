use std::sync::Arc;

use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};

use crate::serialize;
use crate::ResponseBody;

/// The result of a single health check.
#[derive(Debug, Clone, PartialEq)]
pub enum HealthStatus {
    Healthy,
    Degraded(String),
    Unhealthy(String),
}

/// A single health check probe that can be registered with the registry.
pub trait HealthCheck: Send + Sync {
    fn name(&self) -> &'static str;
    fn check(&self) -> impl std::future::Future<Output = HealthStatus> + Send;
}

/// A registered health check with its name and probe function.
struct RegisteredCheck {
    name: &'static str,
    func: Arc<
        dyn Fn() -> std::pin::Pin<Box<dyn std::future::Future<Output = HealthStatus> + Send>>
            + Send
            + Sync,
    >,
}

/// Registry of health checks that can produce a combined health report.
#[derive(Default)]
pub struct HealthRegistry {
    checks: Vec<RegisteredCheck>,
}

impl HealthRegistry {
    pub fn new() -> Self {
        Self { checks: Vec::new() }
    }

    /// Register a health check probe.
    pub fn register(&mut self, check: impl HealthCheck + 'static) {
        let name: &'static str = check.name();
        let check = Arc::new(check);
        let func: Arc<dyn Fn() -> _ + Send + Sync> = Arc::new(move || {
            let check = check.clone();
            Box::pin(async move { check.check().await })
                as std::pin::Pin<Box<dyn std::future::Future<Output = HealthStatus> + Send>>
        });
        self.checks.push(RegisteredCheck { name, func });
    }

    /// Register a health check from a name and async function.
    pub fn register_fn<F, Fut>(&mut self, name: &'static str, f: F)
    where
        F: Fn() -> Fut + Send + Sync + 'static,
        Fut: std::future::Future<Output = HealthStatus> + Send + 'static,
    {
        let func = Arc::new(
            move || -> std::pin::Pin<Box<dyn std::future::Future<Output = HealthStatus> + Send>> {
                Box::pin(f())
            },
        );
        self.checks.push(RegisteredCheck { name, func });
    }

    /// Returns true if no health checks are registered.
    pub fn is_empty(&self) -> bool {
        self.checks.is_empty()
    }

    /// Run all registered checks and return a health report.
    pub async fn check_all(&self) -> HealthReport {
        let mut components = Vec::with_capacity(self.checks.len());
        for check in &self.checks {
            let status = (check.func)().await;
            components.push(ComponentStatus { name: check.name, status });
        }
        HealthReport { components }
    }

    /// Produces an HTTP response for the `/health` endpoint.
    /// Returns 200 if all checks are healthy, 503 if any are unhealthy.
    pub async fn health_response(&self) -> Response<ResponseBody> {
        let report = self.check_all().await;
        let overall_status = report.overall();
        let status_code = match overall_status {
            OverallHealth::Healthy => StatusCode::OK,
            OverallHealth::Degraded => StatusCode::OK,
            OverallHealth::Unhealthy => StatusCode::SERVICE_UNAVAILABLE,
        };

        let body = serialize::to_json_string(&report).unwrap_or_else(|_| {
            r#"{"status":"error","message":"serialization failed"}"#.to_string()
        });

        Response::builder()
            .status(status_code)
            .header("content-type", "application/json")
            .header("content-length", body.len().to_string())
            .body(UnsyncBoxBody::new(
                Full::new(Bytes::from(body))
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))
            .unwrap()
    }
}

/// The overall health status derived from all components.
#[derive(Debug, Clone, PartialEq)]
pub enum OverallHealth {
    Healthy,
    Degraded,
    Unhealthy,
}

/// Status of a single component.
#[derive(Debug, Clone)]
pub struct ComponentStatus {
    pub name: &'static str,
    pub status: HealthStatus,
}

/// Full health report containing all component statuses.
#[derive(Debug, Clone)]
pub struct HealthReport {
    pub components: Vec<ComponentStatus>,
}

impl HealthReport {
    pub fn overall(&self) -> OverallHealth {
        let mut has_degraded = false;
        for c in &self.components {
            match c.status {
                HealthStatus::Unhealthy(_) => return OverallHealth::Unhealthy,
                HealthStatus::Degraded(_) => has_degraded = true,
                HealthStatus::Healthy => {}
            }
        }
        if has_degraded {
            OverallHealth::Degraded
        } else {
            OverallHealth::Healthy
        }
    }
}

use serde::Serialize;

impl Serialize for HealthStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        match self {
            HealthStatus::Healthy => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("status".to_string(), "healthy".to_string());
                m.serialize(s)
            }
            HealthStatus::Degraded(msg) => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("status".to_string(), "degraded".to_string());
                m.insert("message".to_string(), msg.clone());
                m.serialize(s)
            }
            HealthStatus::Unhealthy(msg) => {
                let mut m = std::collections::BTreeMap::new();
                m.insert("status".to_string(), "unhealthy".to_string());
                m.insert("message".to_string(), msg.clone());
                m.serialize(s)
            }
        }
    }
}

impl Serialize for ComponentStatus {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut st = s.serialize_struct("ComponentStatus", 2)?;
        st.serialize_field("name", self.name)?;
        st.serialize_field("status", &self.status)?;
        st.end()
    }
}

impl Serialize for HealthReport {
    fn serialize<S: serde::Serializer>(&self, s: S) -> std::result::Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let overall = self.overall();
        let status_str = match overall {
            OverallHealth::Healthy => "healthy",
            OverallHealth::Degraded => "degraded",
            OverallHealth::Unhealthy => "unhealthy",
        };
        let mut st = s.serialize_struct("HealthReport", 3)?;
        st.serialize_field("status", status_str)?;
        st.serialize_field("components", &self.components)?;
        st.end()
    }
}

/// A health check that always returns healthy (for testing / fallback).
pub struct AlwaysHealthy {
    pub name: &'static str,
}

impl HealthCheck for AlwaysHealthy {
    fn name(&self) -> &'static str {
        self.name
    }
    async fn check(&self) -> HealthStatus {
        HealthStatus::Healthy
    }
}

#[cfg(feature = "db")]
pub mod db_check {
    use super::*;
    use crate::db::AnyPool;

    /// Health check for a database connection pool.
    pub struct DbHealthCheck {
        name: &'static str,
        pool: AnyPool,
    }

    impl DbHealthCheck {
        pub fn new(name: &'static str, pool: AnyPool) -> Self {
            Self { name, pool }
        }
    }

    impl HealthCheck for DbHealthCheck {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn check(&self) -> HealthStatus {
            match self.pool.health_check().await {
                Ok(_) => HealthStatus::Healthy,
                Err(e) => HealthStatus::Unhealthy(format!("database check failed: {}", e)),
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    struct TestCheck {
        name: &'static str,
        result: HealthStatus,
    }

    impl HealthCheck for TestCheck {
        fn name(&self) -> &'static str {
            self.name
        }
        async fn check(&self) -> HealthStatus {
            self.result.clone()
        }
    }

    #[tokio::test]
    async fn test_all_healthy() {
        let mut registry = HealthRegistry::new();
        registry.register(TestCheck { name: "test1", result: HealthStatus::Healthy });
        registry.register(AlwaysHealthy { name: "always" });
        let report = registry.check_all().await;
        assert_eq!(report.overall(), OverallHealth::Healthy);
        assert_eq!(report.components.len(), 2);
    }

    #[tokio::test]
    async fn test_degraded() {
        let mut registry = HealthRegistry::new();
        registry.register(TestCheck {
            name: "test1",
            result: HealthStatus::Degraded("slow".to_string()),
        });
        let report = registry.check_all().await;
        assert_eq!(report.overall(), OverallHealth::Degraded);
    }

    #[tokio::test]
    async fn test_unhealthy() {
        let mut registry = HealthRegistry::new();
        registry.register(TestCheck {
            name: "test1",
            result: HealthStatus::Unhealthy("down".to_string()),
        });
        let report = registry.check_all().await;
        assert_eq!(report.overall(), OverallHealth::Unhealthy);
    }

    #[tokio::test]
    async fn test_health_response() {
        let mut registry = HealthRegistry::new();
        registry.register(AlwaysHealthy { name: "test" });
        let resp = registry.health_response().await;
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_unhealthy_response() {
        let mut registry = HealthRegistry::new();
        registry.register(TestCheck {
            name: "test",
            result: HealthStatus::Unhealthy("crash".to_string()),
        });
        let resp = registry.health_response().await;
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[test]
    fn test_register_fn() {
        let mut registry = HealthRegistry::new();
        registry.register_fn("custom", || async { HealthStatus::Healthy });
        assert_eq!(registry.checks.len(), 1);
    }
}
