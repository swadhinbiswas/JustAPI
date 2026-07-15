use std::sync::Arc;

use hyper::body::Incoming;
use hyper::{Method, Request};

use crate::middleware::HandlerFn;

/// Controls which routes and methods should be audit-logged.
#[derive(Clone)]
pub struct AuditRule {
    methods: Vec<Method>,
    paths: Vec<String>,
}

impl AuditRule {
    pub fn new() -> Self {
        Self { methods: Vec::new(), paths: Vec::new() }
    }

    pub fn method(mut self, method: Method) -> Self {
        self.methods.push(method);
        self
    }

    pub fn path(mut self, path: &str) -> Self {
        self.paths.push(path.to_string());
        self
    }

    fn matches(&self, method: &Method, path: &str) -> bool {
        if !self.methods.is_empty() && !self.methods.iter().any(|m| m == method) {
            return false;
        }
        if !self.paths.is_empty() && !self.paths.iter().any(|p| path.starts_with(p)) {
            return false;
        }
        true
    }
}

impl Default for AuditRule {
    fn default() -> Self {
        Self {
            methods: vec![Method::POST, Method::PUT, Method::DELETE, Method::PATCH],
            paths: Vec::new(),
        }
    }
}

/// Middleware that logs request/response details for sensitive endpoints.
pub struct AuditLogging {
    rule: AuditRule,
}

impl AuditLogging {
    pub fn new(rule: AuditRule) -> Self {
        Self { rule }
    }

    /// Wrap an inner handler so that matched requests are logged with audit metadata.
    pub fn wrap_handler(self, handler: HandlerFn) -> HandlerFn {
        let rule = self.rule;
        Arc::new(move |req: Request<Incoming>| {
            let rule = rule.clone();
            let handler = handler.clone();
            Box::pin(async move {
                let method = req.method().clone();
                let path = req.uri().path().to_string();
                let should_log = rule.matches(&method, &path);

                let start = std::time::Instant::now();
                let resp = handler(req).await;
                let elapsed = start.elapsed();

                if should_log {
                    let status = resp.as_ref().map(|r| r.status().as_u16()).unwrap_or(0);
                    tracing::info!(
                        audit = true,
                        method = %method,
                        path = %path,
                        status = status,
                        duration_us = elapsed.as_micros() as u64,
                        "audit log"
                    );
                }

                resp
            })
        })
    }
}

impl Default for AuditLogging {
    fn default() -> Self {
        Self::new(AuditRule::default())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_audit_rule_matches() {
        let rule = AuditRule::default();
        assert!(rule.matches(&Method::POST, "/api/users"));
        assert!(rule.matches(&Method::DELETE, "/api/users/42"));
        assert!(!rule.matches(&Method::GET, "/health"));
    }

    #[test]
    fn test_audit_rule_custom() {
        let rule = AuditRule::new().method(Method::GET).path("/admin");
        assert!(rule.matches(&Method::GET, "/admin/settings"));
        assert!(!rule.matches(&Method::GET, "/api/users"));
        assert!(!rule.matches(&Method::POST, "/admin"));
    }
}
