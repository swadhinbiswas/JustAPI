use std::collections::HashMap;
use std::net::SocketAddr;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use futures::future::BoxFuture;
use governor::clock::Clock;
use http::HeaderValue;
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use hyper::body::{Bytes, Incoming};
use hyper::{Method, Request, Response, StatusCode};

use crate::ResponseBody;

// ---------------------------------------------------------------------------
// Middleware trait (generic over body type for testability)
// ---------------------------------------------------------------------------

/// Middleware that processes a request and optionally passes it to the next handler.
#[async_trait]
pub trait Middleware<B = Incoming>: Send + Sync {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>>;
}

/// The next element in the middleware chain.
pub struct Next<'a, B = Incoming> {
    middlewares: &'a [Arc<dyn Middleware<B>>],
    index: usize,
    handler: &'a HandlerFn<B>,
}

impl<'a, B> Next<'a, B> {
    pub async fn run(self, req: Request<B>) -> Result<Response<ResponseBody>> {
        if self.index < self.middlewares.len() {
            let mw = &self.middlewares[self.index];
            let next = Next {
                middlewares: self.middlewares,
                index: self.index + 1,
                handler: self.handler,
            };
            let mw_name = std::any::type_name_of_val(mw);
            let span = tracing::debug_span!("middleware", mw.name = %mw_name);
            let _enter = span.enter();
            let result = mw.handle(req, next).await;
            drop(_enter);
            result
        } else {
            let span = tracing::debug_span!("handler.dispatch");
            let _enter = span.enter();
            let result = (self.handler)(req).await;
            drop(_enter);
            result
        }
    }
}

/// Handler function type, generic over body type.
pub type HandlerFn<B = Incoming> =
    Arc<dyn Fn(Request<B>) -> BoxFuture<'static, Result<Response<ResponseBody>>> + Send + Sync>;

// ---------------------------------------------------------------------------
// Middleware chain (generic over body type)
// ---------------------------------------------------------------------------

/// A composable chain of middleware wrapping an inner handler.
pub struct MiddlewareChain<B = Incoming> {
    middlewares: Vec<Arc<dyn Middleware<B>>>,
    handler: HandlerFn<B>,
}

impl<B> Clone for MiddlewareChain<B> {
    fn clone(&self) -> Self {
        Self { middlewares: self.middlewares.clone(), handler: self.handler.clone() }
    }
}

impl<B> MiddlewareChain<B> {
    pub fn new(handler: HandlerFn<B>) -> Self {
        Self { middlewares: Vec::new(), handler }
    }

    pub fn add(&mut self, mw: impl Middleware<B> + 'static) {
        self.middlewares.push(Arc::new(mw));
    }

    pub fn set_handler(&mut self, handler: HandlerFn<B>) {
        self.handler = handler;
    }

    pub fn is_empty(&self) -> bool {
        self.middlewares.is_empty()
    }

    pub async fn run(&self, req: Request<B>) -> Result<Response<ResponseBody>> {
        if self.middlewares.is_empty() {
            return (self.handler)(req).await;
        }
        let next = Next { middlewares: &self.middlewares, index: 0, handler: &self.handler };
        next.run(req).await
    }
}

// ---------------------------------------------------------------------------
// Middleware implementations (concrete, for Incoming body only)
// ---------------------------------------------------------------------------

pub struct AccessLogger {
    tx: tokio::sync::mpsc::UnboundedSender<String>,
}

impl AccessLogger {
    pub fn new() -> Self {
        let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<String>();

        std::thread::spawn(move || {
            while let Some(msg) = rx.blocking_recv() {
                println!("{}", msg);
            }
        });

        Self { tx }
    }
}

impl Default for AccessLogger {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for AccessLogger {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let method = req.method().clone();
        let uri = req.uri().clone();
        let start = std::time::Instant::now();

        let res = next.run(req).await;

        let status = match &res {
            Ok(r) => r.status().as_u16(),
            Err(_) => 500,
        };
        let duration = start.elapsed();

        let _ = self.tx.send(format!("[{method}] {uri} - {status} - {duration:?}"));

        res
    }
}

pub struct Cors {
    allow_origins: Vec<String>,
    allow_methods: String,
    allow_headers: String,
    expose_headers: Vec<String>,
    max_age: String,
    allow_credentials: bool,
}

impl Cors {
    pub fn permissive() -> Self {
        Self {
            allow_origins: vec!["*".to_string()],
            allow_methods: "GET, POST, PUT, DELETE, PATCH, OPTIONS".to_string(),
            allow_headers: "Content-Type, Authorization".to_string(),
            expose_headers: Vec::new(),
            max_age: "86400".to_string(),
            allow_credentials: false,
        }
    }

    pub fn new() -> Self {
        Self {
            // Secure-by-default: no `Access-Control-Allow-Origin` is emitted
            // until the caller configures explicit origins via `allow_origin`.
            // Browsers then enforce same-origin. Use `permissive()` for the old
            // open `*` behavior.
            allow_origins: Vec::new(),
            allow_methods: "GET, POST, PUT, DELETE, PATCH, OPTIONS".to_string(),
            allow_headers: "Content-Type, Authorization".to_string(),
            expose_headers: Vec::new(),
            max_age: "86400".to_string(),
            allow_credentials: false,
        }
    }

    pub fn allow_origin(mut self, origin: &str) -> Self {
        if origin == "*" {
            self.allow_origins = vec!["*".to_string()];
        } else if self.allow_origins == ["*"] {
            self.allow_origins = vec![origin.to_string()];
        } else {
            self.allow_origins.push(origin.to_string());
        }
        self
    }

    pub fn allow_methods(mut self, methods: &str) -> Self {
        self.allow_methods = methods.to_string();
        self
    }

    pub fn allow_headers(mut self, headers: &str) -> Self {
        self.allow_headers = headers.to_string();
        self
    }

    pub fn expose_headers(mut self, headers: &[&str]) -> Self {
        self.expose_headers = headers.iter().map(|h| h.to_string()).collect();
        self
    }

    pub fn allow_credentials(mut self) -> Self {
        self.allow_credentials = true;
        self
    }

    pub fn max_age(mut self, seconds: &str) -> Self {
        self.max_age = seconds.to_string();
        self
    }
}

impl Default for Cors {
    fn default() -> Self {
        // Secure-by-default: same-origin only until configured.
        Self::new()
    }
}

fn origin_matches(origin: &str, allowed: &[String]) -> bool {
    allowed.iter().any(|a| a == "*" || a == origin)
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for Cors {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let req_origin = req.headers().get("origin").and_then(|v| v.to_str().ok());

        let allow_all = self.allow_origins.iter().any(|a| a == "*");

        // When credentials are allowed, the wildcard "*" must NOT be echoed:
        // browsers reject `Access-Control-Allow-Origin: *` together with
        // `Access-Control-Allow-Credentials: true`, and echoing "*" with
        // credentials would be insecure. Reflect the concrete request origin
        // instead (only when one is present).
        let matched_origin = if allow_all {
            if self.allow_credentials {
                match req_origin {
                    Some(o) => o.to_string(),
                    None => return next.run(req).await,
                }
            } else {
                "*".to_string()
            }
        } else if let Some(origin) = req_origin {
            if origin_matches(origin, &self.allow_origins) {
                origin.to_string()
            } else {
                return next.run(req).await;
            }
        } else {
            return next.run(req).await;
        };

        let add_vary = !allow_all;

        if req.method() == Method::OPTIONS {
            let origin_val: HeaderValue = matched_origin
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid CORS origin header value: {e}"))?;
            let mut builder = Response::builder()
                .status(StatusCode::NO_CONTENT)
                .header("access-control-allow-origin", origin_val)
                .header("access-control-allow-methods", &self.allow_methods)
                .header("access-control-allow-headers", &self.allow_headers)
                .header("access-control-max-age", &self.max_age);

            if self.allow_credentials {
                builder = builder.header("access-control-allow-credentials", "true");
            }
            if !self.expose_headers.is_empty() {
                builder =
                    builder.header("access-control-expose-headers", self.expose_headers.join(", "));
            }
            if add_vary {
                builder = builder.header("vary", "Origin");
            }

            let resp = builder
                .body(UnsyncBoxBody::new(
                    http_body_util::Full::new(Bytes::new())
                        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                ))
                .unwrap();
            return Ok(resp);
        }

        let mut resp = next.run(req).await?;
        let origin_val: HeaderValue = matched_origin
            .parse()
            .map_err(|e| anyhow::anyhow!("invalid CORS origin header value: {e}"))?;
        resp.headers_mut().insert("access-control-allow-origin", origin_val);
        if self.allow_credentials {
            resp.headers_mut()
                .insert("access-control-allow-credentials", HeaderValue::from_static("true"));
        }
        if !self.expose_headers.is_empty() {
            let val: HeaderValue = self
                .expose_headers
                .join(", ")
                .parse()
                .map_err(|e| anyhow::anyhow!("invalid CORS expose-headers value: {e}"))?;
            resp.headers_mut().insert("access-control-expose-headers", val);
        }
        if add_vary {
            resp.headers_mut().insert("vary", HeaderValue::from_static("Origin"));
        }
        Ok(resp)
    }
}

pub struct SecurityHeaders {
    hsts: String,
    csp_directives: Vec<String>,
    include_xfo: bool,
    include_csp: bool,
    include_hsts: bool,
    include_hsts_preload: bool,
}

impl Default for SecurityHeaders {
    fn default() -> Self {
        Self {
            hsts: "max-age=31536000; includeSubDomains".to_string(),
            csp_directives: vec!["default-src 'self'".to_string()],
            include_xfo: true,
            include_csp: true,
            include_hsts: true,
            include_hsts_preload: false,
        }
    }
}

impl SecurityHeaders {
    pub fn with_hsts_preload(mut self) -> Self {
        self.hsts = format!("{}; preload", self.hsts.trim_end_matches("; preload"));
        self.include_hsts_preload = true;
        self
    }

    pub fn with_csp_directive(mut self, directive: &str) -> Self {
        self.csp_directives.push(directive.to_string());
        self
    }

    pub fn without_xfo(mut self) -> Self {
        self.include_xfo = false;
        self
    }

    pub fn without_csp(mut self) -> Self {
        self.include_csp = false;
        self
    }

    /// Omit `Strict-Transport-Security`. Use for plaintext (non-TLS) deployments:
    /// emitting HSTS over HTTP is both useless and can pin a dev environment to
    /// HTTPS that it cannot serve.
    pub fn without_hsts(mut self) -> Self {
        self.include_hsts = false;
        self
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for SecurityHeaders {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let mut resp = next.run(req).await?;
        let headers = resp.headers_mut();

        headers.insert("x-content-type-options", "nosniff".parse().unwrap());

        if self.include_xfo {
            headers.insert("x-frame-options", "DENY".parse().unwrap());
        }

        let hsts_val = if self.include_hsts_preload {
            format!("{}; preload", self.hsts.trim_end_matches("; preload"))
        } else {
            self.hsts.clone()
        };
        if self.include_hsts {
            headers.insert("strict-transport-security", hsts_val.parse().unwrap());
        }

        if self.include_csp {
            let csp = self.csp_directives.join("; ");
            headers.insert("content-security-policy", csp.parse().unwrap());
        }

        headers.insert("x-xss-protection", "0".parse().unwrap());
        Ok(resp)
    }
}

/// Per-route JWT requirement.
#[derive(Clone, Debug)]
pub enum JwtRequirement {
    /// No JWT validation required for this route.
    None,
    /// JWT required; no additional claims checked.
    Required,
    /// JWT required AND the token's `roles` claim must include at least one of these.
    Roles(Vec<String>),
    /// JWT required AND the token's `scope` or `scopes` claim must include at least one of these.
    Scopes(Vec<String>),
}

pub struct JwtAuth {
    validation: jsonwebtoken::Validation,
    decoding_key: jsonwebtoken::DecodingKey,
    route_rules: Vec<(String, JwtRequirement)>,
    default_requirement: JwtRequirement,
    warned_aud_iss: std::sync::atomic::AtomicBool,
}

impl std::fmt::Debug for JwtAuth {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("JwtAuth")
            .field("default_requirement", &self.default_requirement)
            .field("route_rules", &self.route_rules)
            .finish()
    }
}

impl JwtAuth {
    pub fn from_secret(secret: &[u8]) -> Self {
        Self {
            validation: jsonwebtoken::Validation::default(),
            decoding_key: jsonwebtoken::DecodingKey::from_secret(secret),
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn from_rsa_pem(pem: &str) -> anyhow::Result<Self> {
        let decoding_key = jsonwebtoken::DecodingKey::from_rsa_pem(pem.as_bytes())?;
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.algorithms = vec![
            jsonwebtoken::Algorithm::RS256,
            jsonwebtoken::Algorithm::RS384,
            jsonwebtoken::Algorithm::RS512,
        ];
        Ok(Self {
            validation,
            decoding_key,
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn from_ec_pem(pem: &str) -> anyhow::Result<Self> {
        let decoding_key = jsonwebtoken::DecodingKey::from_ec_pem(pem.as_bytes())?;
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.algorithms =
            vec![jsonwebtoken::Algorithm::ES256, jsonwebtoken::Algorithm::ES384];
        Ok(Self {
            validation,
            decoding_key,
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        })
    }

    pub fn from_rsa_der(der: &[u8]) -> Self {
        let decoding_key = jsonwebtoken::DecodingKey::from_rsa_der(der);
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::RS256);
        validation.algorithms = vec![
            jsonwebtoken::Algorithm::RS256,
            jsonwebtoken::Algorithm::RS384,
            jsonwebtoken::Algorithm::RS512,
        ];
        Self {
            validation,
            decoding_key,
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn from_ec_der(der: &[u8]) -> Self {
        let decoding_key = jsonwebtoken::DecodingKey::from_ec_der(der);
        let mut validation = jsonwebtoken::Validation::new(jsonwebtoken::Algorithm::ES256);
        validation.algorithms =
            vec![jsonwebtoken::Algorithm::ES256, jsonwebtoken::Algorithm::ES384];
        Self {
            validation,
            decoding_key,
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        }
    }

    pub fn with_algorithm(mut self, alg: jsonwebtoken::Algorithm) -> Self {
        self.validation.algorithms = vec![alg];
        self
    }

    pub fn with_algorithms(mut self, algs: Vec<jsonwebtoken::Algorithm>) -> Self {
        self.validation.algorithms = algs;
        self
    }

    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.validation.iss = Some([issuer.to_string()].into());
        self
    }

    pub fn with_audience(mut self, audience: &[&str]) -> Self {
        self.validation.aud = Some(audience.iter().map(|a| a.to_string()).collect());
        self
    }

    /// Require JWT validation for requests whose path starts with the given prefix.
    pub fn require_for(mut self, path_prefix: &str, requirement: JwtRequirement) -> Self {
        self.route_rules.push((path_prefix.to_string(), requirement));
        self
    }

    /// Set the default requirement for routes without an explicit rule.
    pub fn default_requirement(mut self, requirement: JwtRequirement) -> Self {
        self.default_requirement = requirement;
        self
    }

    fn requirement_for_path(&self, path: &str) -> &JwtRequirement {
        for (prefix, req) in &self.route_rules {
            if path.starts_with(prefix) {
                return req;
            }
        }
        &self.default_requirement
    }
}

fn check_claims(
    claims: &serde_json::Value,
    requirement: &JwtRequirement,
) -> Result<(), &'static str> {
    match requirement {
        JwtRequirement::None => Ok(()),
        JwtRequirement::Required => Ok(()),
        JwtRequirement::Roles(required_roles) => {
            let roles = claims.get("roles").and_then(|v| v.as_array());
            match roles {
                Some(role_list) => {
                    let user_roles: Vec<&str> =
                        role_list.iter().filter_map(|v| v.as_str()).collect();
                    if required_roles.iter().any(|r| user_roles.contains(&r.as_str())) {
                        Ok(())
                    } else {
                        Err("missing required role")
                    }
                }
                None => Err("missing roles claim"),
            }
        }
        JwtRequirement::Scopes(required_scopes) => {
            let scopes = claims
                .get("scope")
                .or_else(|| claims.get("scopes"))
                .and_then(|v| v.as_str())
                .map(|s| s.split_whitespace().collect::<Vec<&str>>())
                .or_else(|| {
                    claims
                        .get("scope")
                        .or_else(|| claims.get("scopes"))
                        .and_then(|v| v.as_array())
                        .map(|arr| arr.iter().filter_map(|v| v.as_str()).collect())
                });
            match scopes {
                Some(user_scopes) => {
                    if required_scopes.iter().any(|s| user_scopes.contains(&s.as_str())) {
                        Ok(())
                    } else {
                        Err("missing required scope")
                    }
                }
                None => Err("missing scope/scopes claim"),
            }
        }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for JwtAuth {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        // One-time warning if aud/iss validation is not configured.
        if self.validation.aud.is_none()
            && self.validation.iss.is_none()
            && !self.warned_aud_iss.swap(true, std::sync::atomic::Ordering::Relaxed)
        {
            tracing::warn!(
                "JwtAuth: neither `aud` nor `iss` claim is validated. \
                 Call `.with_audience([...])` and/or `.with_issuer(...)` \
                 to prevent token replay across different services."
            );
        }

        let path = req.uri().path().to_string();
        let requirement = self.requirement_for_path(&path);

        // If no JWT required for this route, skip validation entirely
        if matches!(requirement, JwtRequirement::None) {
            return next.run(req).await;
        }

        let auth_header = req.headers().get("authorization").and_then(|v| v.to_str().ok());
        let token = match auth_header {
            Some(h) if h.starts_with("Bearer ") => &h[7..],
            _ => {
                return Ok(json_error(
                    StatusCode::UNAUTHORIZED,
                    "missing or invalid authorization header",
                ))
            }
        };

        match jsonwebtoken::decode::<serde_json::Value>(token, &self.decoding_key, &self.validation)
        {
            Ok(token_data) => {
                if let Err(msg) = check_claims(&token_data.claims, requirement) {
                    return Ok(json_error(StatusCode::FORBIDDEN, msg));
                }
                let mut req = req;
                req.extensions_mut().insert(token_data.claims);
                next.run(req).await
            }
            Err(_) => Ok(json_error(StatusCode::UNAUTHORIZED, "invalid token")),
        }
    }
}

pub struct RateLimiter {
    limiter: governor::RateLimiter<
        governor::state::direct::NotKeyed,
        governor::state::InMemoryState,
        governor::clock::DefaultClock,
    >,
}

impl RateLimiter {
    pub fn new(duration: std::time::Duration, max_burst: u32) -> Self {
        let quota = governor::Quota::with_period(duration)
            .unwrap()
            .allow_burst(NonZeroU32::new(max_burst).unwrap());
        Self { limiter: governor::RateLimiter::direct(quota) }
    }

    pub fn per_second(max_burst: u32) -> Self {
        Self::new(std::time::Duration::from_secs(1), max_burst)
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for RateLimiter {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        match self.limiter.check() {
            Ok(()) => next.run(req).await,
            Err(negative) => {
                let clock = governor::clock::DefaultClock::default();
                let now = clock.now();
                let wait = negative.wait_time_from(now);
                let retry_after_secs = wait.as_secs().max(1);
                let mut resp = json_error(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
                resp.headers_mut()
                    .insert("retry-after", retry_after_secs.to_string().parse().unwrap());
                resp.headers_mut()
                    .insert("x-ratelimit-reset", retry_after_secs.to_string().parse().unwrap());
                Ok(resp)
            }
        }
    }
}

/// Per-IP rate limiter using governor's keyed rate limiter.
/// Extracts client IP from connection info (requires `req.extensions().get::<SocketAddr>()`).
pub struct IpRateLimiter {
    limiter: std::sync::Arc<
        governor::RateLimiter<
            String,
            governor::state::keyed::DefaultKeyedStateStore<String>,
            governor::clock::DefaultClock,
        >,
    >,
}

impl IpRateLimiter {
    pub fn new(duration: std::time::Duration, max_burst: u32) -> Self {
        let quota = governor::Quota::with_period(duration)
            .unwrap()
            .allow_burst(NonZeroU32::new(max_burst).unwrap());
        let limiter = std::sync::Arc::new(governor::RateLimiter::keyed(quota));

        let limiter_clone = limiter.clone();
        tokio::spawn(async move {
            let mut interval = tokio::time::interval(std::time::Duration::from_secs(60));
            loop {
                interval.tick().await;
                limiter_clone.retain_recent();
            }
        });

        Self { limiter }
    }

    pub fn per_second(max_burst: u32) -> Self {
        Self::new(std::time::Duration::from_secs(1), max_burst)
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for IpRateLimiter {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let ip = req
            .extensions()
            .get::<SocketAddr>()
            .map(|a| a.ip().to_string())
            .unwrap_or_else(|| "unknown".to_string());

        match self.limiter.check_key(&ip) {
            Ok(()) => next.run(req).await,
            Err(negative) => {
                let clock = governor::clock::DefaultClock::default();
                let now = clock.now();
                let wait = negative.wait_time_from(now);
                let retry_after_secs = wait.as_secs().max(1);
                let mut resp = json_error(StatusCode::TOO_MANY_REQUESTS, "rate limit exceeded");
                resp.headers_mut()
                    .insert("retry-after", retry_after_secs.to_string().parse().unwrap());
                resp.headers_mut()
                    .insert("x-ratelimit-reset", retry_after_secs.to_string().parse().unwrap());
                Ok(resp)
            }
        }
    }
}

fn json_error(status: StatusCode, msg: &str) -> Response<ResponseBody> {
    let body = format!(r#"{{"error":"{}"}}"#, msg);
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(
            http_body_util::Full::new(Bytes::from(body))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

// ---------------------------------------------------------------------------
// API-key authentication (the "API-key scheme" from OpenAPI / FastAPI)
// ---------------------------------------------------------------------------

/// API-key authentication middleware.
///
/// Validates a key presented in a configurable request header (default
/// `x-api-key`) or, optionally, a query parameter. Valid keys map to a set of
/// claims (subject / roles / scopes) inserted into the request extensions for
/// downstream handlers. Per-path-prefix rules control whether a key is required
/// at all (e.g. public health endpoints vs. private `/v1/*` routes).
pub struct ApiKeyAuth {
    header_name: hyper::header::HeaderName,
    query_param: Option<String>,
    keys: std::collections::HashMap<String, serde_json::Value>,
    route_rules: Vec<(String, bool)>,
    default_required: bool,
}

impl ApiKeyAuth {
    /// Create an API-key authenticator reading keys from `header_name`
    /// (e.g. `"x-api-key"`). No keys are valid until registered with
    /// [`ApiKeyAuth::with_key`].
    pub fn new(header_name: &str) -> Self {
        Self {
            header_name: hyper::header::HeaderName::from_bytes(header_name.as_bytes())
                .expect("valid header name"),
            query_param: None,
            keys: std::collections::HashMap::new(),
            route_rules: Vec::new(),
            default_required: true,
        }
    }

    /// Register a valid key with its associated claims (subject / roles / scopes).
    pub fn with_key(mut self, key: &str, claims: serde_json::Value) -> Self {
        self.keys.insert(key.to_string(), claims);
        self
    }

    /// Also accept the key via this query parameter (in addition to the header).
    pub fn with_query_param(mut self, name: &str) -> Self {
        self.query_param = Some(name.to_string());
        self
    }

    /// Require (or exempt) keys for paths starting with `path_prefix`.
    pub fn require_for(mut self, path_prefix: &str, required: bool) -> Self {
        self.route_rules.push((path_prefix.to_string(), required));
        self
    }

    /// Set whether keys are required by default for paths without an explicit rule.
    pub fn default_required(mut self, required: bool) -> Self {
        self.default_required = required;
        self
    }

    fn required_for_path(&self, path: &str) -> bool {
        for (prefix, req) in &self.route_rules {
            if path.starts_with(prefix) {
                return *req;
            }
        }
        self.default_required
    }

    fn extract_key<B>(&self, req: &Request<B>) -> Option<String> {
        if let Some(v) = req.headers().get(&self.header_name).and_then(|v| v.to_str().ok()) {
            return Some(v.to_string());
        }
        if let Some(qp) = &self.query_param {
            if let Some(query) = req.uri().query() {
                for pair in query.split('&') {
                    let mut it = pair.splitn(2, '=');
                    if let (Some(k), Some(v)) = (it.next(), it.next()) {
                        if k == qp {
                            return Some(v.to_string());
                        }
                    }
                }
            }
        }
        None
    }

    async fn handle_inner<B: Send + 'static>(
        &self,
        req: Request<B>,
        next: Next<'_, B>,
    ) -> Result<Response<ResponseBody>> {
        let path = req.uri().path().to_string();
        if !self.required_for_path(&path) {
            return next.run(req).await;
        }
        match self.extract_key(&req) {
            Some(key) => {
                if let Some(claims) = self.keys.get(&key) {
                    let mut req = req;
                    req.extensions_mut().insert(claims.clone());
                    next.run(req).await
                } else {
                    Ok(json_error(StatusCode::UNAUTHORIZED, "invalid api key"))
                }
            }
            None => Ok(json_error(StatusCode::UNAUTHORIZED, "missing api key")),
        }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for ApiKeyAuth {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        self.handle_inner(req, next).await
    }
}

// ---------------------------------------------------------------------------
// OAuth2 Password flow — token issuance
// ---------------------------------------------------------------------------

/// User validator for the OAuth2 password flow.
pub type UserValidator = Arc<dyn Fn(&str, &str) -> Option<serde_json::Value> + Send + Sync>;

/// OAuth2 Password-flow provider.
///
/// Issues signed JWTs in exchange for valid `username`/`password` credentials
/// (the Resource Owner Password Credentials Grant, RFC 6749 §4.3). The returned
/// token handler can be mounted at any path (default `/token`). Use
/// [`OAuth2Password::jwt_auth`] to obtain a pre-configured [`JwtAuth`] that
/// verifies the issued tokens.
///
/// # Example
///
/// ```ignore
/// let oauth2 = OAuth2Password::new(
///     |user, pass| {
///         if user == "alice" && pass == "secret" {
///             Some(serde_json::json!({"sub": "alice", "roles": ["user"]}))
///         } else { None }
///     },
///     jsonwebtoken::EncodingKey::from_secret(b"my_secret"),
///     jsonwebtoken::DecodingKey::from_secret(b"my_secret"),
/// );
/// // Register token endpoint + add JwtAuth middleware:
/// // server.with_oauth2_password(oauth2);
/// ```
pub struct OAuth2Password {
    validate_user: UserValidator,
    encoding_key: jsonwebtoken::EncodingKey,
    decoding_key: jsonwebtoken::DecodingKey,
    algorithm: jsonwebtoken::Algorithm,
    pub(crate) token_path: String,
    token_expiry: Duration,
    issuer: Option<String>,
}

impl OAuth2Password {
    /// Create a new OAuth2 password-flow provider.
    ///
    /// `validate_user` is called with `(username, password)` and should return
    /// `Some(claims)` on success (the `sub` claim is typically included) or
    /// `None` on invalid credentials.
    ///
    /// For symmetric (HS*) keys, the same secret bytes work for both encoding
    /// and decoding. For asymmetric (RS*/ES*) algorithms, provide the private
    /// key as `encoding_key` and the public key as `decoding_key`.
    pub fn new(
        validate_user: UserValidator,
        encoding_key: jsonwebtoken::EncodingKey,
        decoding_key: jsonwebtoken::DecodingKey,
    ) -> Self {
        Self {
            validate_user,
            encoding_key,
            decoding_key,
            algorithm: jsonwebtoken::Algorithm::HS256,
            token_path: "/token".to_string(),
            token_expiry: Duration::from_secs(3600), // 1 hour
            issuer: None,
        }
    }

    /// Set the signing algorithm (default `HS256`).
    pub fn with_algorithm(mut self, alg: jsonwebtoken::Algorithm) -> Self {
        self.algorithm = alg;
        self
    }

    /// Set the token endpoint path (default `/token`).
    pub fn with_token_path(mut self, path: &str) -> Self {
        self.token_path = path.to_string();
        self
    }

    /// Set the access-token expiry (default 1 hour).
    pub fn with_token_expiry(mut self, expiry: Duration) -> Self {
        self.token_expiry = expiry;
        self
    }

    /// Set the `iss` (issuer) claim included in issued tokens.
    pub fn with_issuer(mut self, issuer: &str) -> Self {
        self.issuer = Some(issuer.to_string());
        self
    }

    /// Return a handler for the OAuth2 token endpoint, generic over body type
    /// (works with `Incoming` in production and `Full<Bytes>` in tests).
    ///
    /// The handler expects `POST` with `application/x-www-form-urlencoded` body
    /// containing `grant_type=password`, `username`, and `password` fields.
    /// On success it returns `{"access_token": "...", "token_type": "bearer"}`.
    pub fn token_handler<B>(&self) -> HandlerFn<B>
    where
        B: http_body::Body<Data = Bytes> + Send + 'static,
        B::Error: std::error::Error + Send + Sync + 'static,
    {
        let validate_user = self.validate_user.clone();
        let encoding_key_ref = self.encoding_key.clone();
        let algorithm = self.algorithm;
        let token_expiry = self.token_expiry;
        let issuer = self.issuer.clone();
        let path = self.token_path.clone();

        Arc::new(move |req: Request<B>| {
            let validate_user = validate_user.clone();
            let encoding_key_ref = encoding_key_ref.clone();
            let algorithm = algorithm;
            let token_expiry = token_expiry;
            let issuer = issuer.clone();
            let path = path.clone();

            Box::pin(async move {
                if req.method() != Method::POST {
                    return Ok(json_error(StatusCode::METHOD_NOT_ALLOWED, "only POST is allowed"));
                }

                if req.uri().path() != path {
                    return Ok(json_error(StatusCode::NOT_FOUND, "not found"));
                }

                let content_type =
                    req.headers().get("content-type").and_then(|v| v.to_str().ok()).unwrap_or("");
                if !content_type.starts_with("application/x-www-form-urlencoded") {
                    return Ok(json_error(
                        StatusCode::UNSUPPORTED_MEDIA_TYPE,
                        "expected application/x-www-form-urlencoded",
                    ));
                }

                let body_bytes = req.collect().await.map_err(|e| anyhow::anyhow!(e))?.to_bytes();
                let body_str = String::from_utf8_lossy(&body_bytes).to_string();

                let params: HashMap<String, String> = serde_urlencoded::from_str(&body_str)
                    .map_err(|_| anyhow::anyhow!("bad form"))?;

                let grant_type = params.get("grant_type").map(|s| s.as_str()).unwrap_or("");
                if grant_type != "password" {
                    return Ok(json_error(
                        StatusCode::BAD_REQUEST,
                        "unsupported grant_type, expected 'password'",
                    ));
                }

                let username = params.get("username").map(|s| s.as_str()).unwrap_or("");
                let password = params.get("password").map(|s| s.as_str()).unwrap_or("");

                if username.is_empty() || password.is_empty() {
                    return Ok(json_error(
                        StatusCode::BAD_REQUEST,
                        "username and password are required",
                    ));
                }

                let claims_value = match (validate_user)(username, password) {
                    Some(c) => c,
                    None => {
                        return Ok(json_error(
                            StatusCode::UNAUTHORIZED,
                            "invalid username or password",
                        ))
                    }
                };

                let sub = claims_value
                    .get("sub")
                    .and_then(|v| v.as_str())
                    .unwrap_or(username)
                    .to_string();

                let now = std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .unwrap()
                    .as_secs();

                let mut jwt_claims = serde_json::Map::new();
                jwt_claims.insert("sub".to_string(), serde_json::Value::String(sub));
                jwt_claims.insert(
                    "iat".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(now)),
                );
                jwt_claims.insert(
                    "exp".to_string(),
                    serde_json::Value::Number(serde_json::Number::from(
                        now + token_expiry.as_secs(),
                    )),
                );
                if let Some(iss) = &issuer {
                    jwt_claims.insert("iss".to_string(), serde_json::Value::String(iss.clone()));
                }
                // Merge user claims
                if let Some(obj) = claims_value.as_object() {
                    for (k, v) in obj {
                        if !["sub", "iat", "exp", "iss"].contains(&k.as_str()) {
                            jwt_claims.insert(k.clone(), v.clone());
                        }
                    }
                }

                let header = jsonwebtoken::Header::new(algorithm);
                let token = jsonwebtoken::encode(
                    &header,
                    &serde_json::Value::Object(jwt_claims),
                    &encoding_key_ref,
                )
                .map_err(|e| anyhow::anyhow!("token encoding failed: {}", e))?;

                let body = serde_json::json!({
                    "access_token": token,
                    "token_type": "bearer",
                    "expires_in": token_expiry.as_secs(),
                });
                let body_bytes = serde_json::to_vec(&body)?;
                let resp = Response::builder()
                    .status(StatusCode::OK)
                    .header("content-type", "application/json")
                    .header("content-length", body_bytes.len().to_string())
                    .header("cache-control", "no-store")
                    .body(UnsyncBoxBody::new(
                        http_body_util::Full::new(Bytes::from(body_bytes))
                            .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
                    ))
                    .unwrap();
                Ok(resp)
            })
        })
    }

    /// Obtain a [`JwtAuth`] pre-configured to verify tokens issued by this
    /// provider. You may add path rules via the returned builder.
    pub fn jwt_auth(&self) -> JwtAuth {
        let mut validation = jsonwebtoken::Validation::new(self.algorithm);
        if let Some(iss) = &self.issuer {
            validation.iss = Some([iss.clone()].into());
        }
        JwtAuth {
            validation,
            decoding_key: self.decoding_key.clone(),
            route_rules: Vec::new(),
            default_requirement: JwtRequirement::Required,
            warned_aud_iss: std::sync::atomic::AtomicBool::new(false),
        }
    }
}

// ---------------------------------------------------------------------------
// Tests — use Request<http_body_util::Full<Bytes>> for constructability
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_response;

    type TestBody = http_body_util::Full<Bytes>;

    fn test_req(method: Method, uri: &str) -> Request<TestBody> {
        Request::builder().method(method).uri(uri).body(TestBody::new(Bytes::new())).unwrap()
    }

    fn test_req_with_header(method: Method, uri: &str, key: &str, val: &str) -> Request<TestBody> {
        let mut req = test_req(method, uri);
        req.headers_mut().insert(
            hyper::header::HeaderName::from_bytes(key.as_bytes()).unwrap(),
            hyper::header::HeaderValue::from_str(val).unwrap(),
        );
        req
    }

    #[tokio::test]
    async fn test_cors_preflight() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::permissive());
        let req = test_req_with_header(Method::OPTIONS, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(resp.headers()["access-control-allow-origin"], "*");
    }

    #[tokio::test]
    async fn test_cors_headers_added() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::permissive());
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["access-control-allow-origin"], "*");
    }

    #[tokio::test]
    async fn test_security_headers() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "hi")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(SecurityHeaders::default());
        let req = test_req(Method::GET, "/hello");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["x-content-type-options"], "nosniff");
        assert_eq!(resp.headers()["x-frame-options"], "DENY");
    }

    #[tokio::test]
    async fn test_jwt_missing_header() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "hi")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(JwtAuth::from_secret(b"secret"));
        let req = test_req(Method::GET, "/hello");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_invalid_token() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "hi")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(JwtAuth::from_secret(b"secret"));
        let req = test_req_with_header(
            Method::GET,
            "/hello",
            "authorization",
            "Bearer invalid.token.here",
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_valid_token() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "hi")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(JwtAuth::from_secret(b"secret"));
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &serde_json::json!({"sub": "123", "exp": 9999999999u64}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let req = test_req_with_header(
            Method::GET,
            "/hello",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_multiple_middlewares() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "hi")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::permissive());
        chain.add(SecurityHeaders::default());
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["access-control-allow-origin"], "*");
        assert_eq!(resp.headers()["x-content-type-options"], "nosniff");
    }

    #[tokio::test]
    async fn test_middleware_chain_no_middleware() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "direct")) })
        });
        let chain = MiddlewareChain::new(handler);
        let req = test_req(Method::GET, "/hello");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // --- Enhanced Cors tests -------------------------------------------------

    #[tokio::test]
    async fn test_cors_specific_origin_allowed() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::new().allow_origin("https://example.com"));
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["access-control-allow-origin"], "https://example.com");
        assert_eq!(resp.headers()["vary"], "Origin");
    }

    #[tokio::test]
    async fn test_cors_specific_origin_rejected() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::new().allow_origin("https://example.com"));
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://evil.com");
        let resp = chain.run(req).await.unwrap();
        assert!(!resp.headers().contains_key("access-control-allow-origin"));
    }

    #[tokio::test]
    async fn test_cors_credentials() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(Cors::new().allow_origin("https://example.com").allow_credentials());
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["access-control-allow-credentials"], "true");
    }

    #[tokio::test]
    async fn test_cors_expose_headers() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            Cors::new()
                .allow_origin("https://example.com")
                .expose_headers(&["x-custom-header", "x-trace-id"]),
        );
        let req = test_req_with_header(Method::GET, "/hello", "origin", "https://example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.headers()["access-control-expose-headers"], "x-custom-header, x-trace-id");
    }

    #[tokio::test]
    async fn test_cors_preflight_with_credentials() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            Cors::new()
                .allow_origin("https://app.example.com")
                .allow_credentials()
                .allow_methods("POST, OPTIONS")
                .allow_headers("Authorization, Content-Type"),
        );
        let req =
            test_req_with_header(Method::OPTIONS, "/hello", "origin", "https://app.example.com");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::NO_CONTENT);
        assert_eq!(resp.headers()["access-control-allow-origin"], "https://app.example.com");
        assert_eq!(resp.headers()["access-control-allow-credentials"], "true");
        assert_eq!(resp.headers()["vary"], "Origin");
    }

    // --- Enhanced JwtAuth tests ---------------------------------------------

    #[tokio::test]
    async fn test_jwt_no_auth_for_public_route() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "public")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::Required)
                .require_for("/public", JwtRequirement::None),
        );
        // No auth header — should pass for /public
        let req = test_req(Method::GET, "/public/info");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jwt_required_for_protected_route() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "protected")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::Required)
                .require_for("/public", JwtRequirement::None),
        );
        // No auth header for /admin — should fail
        let req = test_req(Method::GET, "/admin/data");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn test_jwt_roles_check() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "admin only")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::None)
                .require_for("/admin", JwtRequirement::Roles(vec!["admin".to_string()])),
        );
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &serde_json::json!({"sub": "123", "roles": ["admin"], "exp": 9999999999u64}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let req = test_req_with_header(
            Method::GET,
            "/admin/data",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jwt_roles_missing() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "admin only")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::None)
                .require_for("/admin", JwtRequirement::Roles(vec!["admin".to_string()])),
        );
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &serde_json::json!({"sub": "123", "roles": ["user"], "exp": 9999999999u64}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let req = test_req_with_header(
            Method::GET,
            "/admin/data",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    #[tokio::test]
    async fn test_jwt_scopes_check() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "scoped")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::None)
                .require_for("/api", JwtRequirement::Scopes(vec!["read:data".to_string()])),
        );
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &serde_json::json!({"sub": "123", "scope": "read:data write:data", "exp": 9999999999u64}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let req = test_req_with_header(
            Method::GET,
            "/api/resource",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_jwt_scopes_missing() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "scoped")) })
        });
        let mut chain = MiddlewareChain::new(handler);
        chain.add(
            JwtAuth::from_secret(b"secret")
                .default_requirement(JwtRequirement::None)
                .require_for("/api", JwtRequirement::Scopes(vec!["admin:all".to_string()])),
        );
        let token = jsonwebtoken::encode(
            &jsonwebtoken::Header::default(),
            &serde_json::json!({"sub": "123", "scope": "read:data", "exp": 9999999999u64}),
            &jsonwebtoken::EncodingKey::from_secret(b"secret"),
        )
        .unwrap();
        let req = test_req_with_header(
            Method::GET,
            "/api/resource",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::FORBIDDEN);
    }

    // --- ApiKeyAuth tests ------------------------------------------------

    fn api_chain(handler: HandlerFn<TestBody>, auth: ApiKeyAuth) -> MiddlewareChain<TestBody> {
        let mut chain = MiddlewareChain::new(handler);
        chain.add(auth);
        chain
    }

    #[tokio::test]
    async fn api_key_valid_header_passes() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let auth = ApiKeyAuth::new("x-api-key")
            .with_key("secret-123", serde_json::json!({"sub": "user-1", "roles": ["admin"]}));
        let chain = api_chain(handler, auth);
        let req = test_req_with_header(Method::GET, "/v1/models", "x-api-key", "secret-123");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_key_missing_is_401() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let auth = ApiKeyAuth::new("x-api-key").with_key("secret-123", serde_json::json!({}));
        let chain = api_chain(handler, auth);
        let req = test_req(Method::GET, "/v1/models");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_key_invalid_is_401() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let auth = ApiKeyAuth::new("x-api-key").with_key("secret-123", serde_json::json!({}));
        let chain = api_chain(handler, auth);
        let req = test_req_with_header(Method::GET, "/v1/models", "x-api-key", "wrong");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_key_query_param_accepted() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let auth = ApiKeyAuth::new("x-api-key")
            .with_query_param("api_key")
            .with_key("secret-123", serde_json::json!({}));
        let chain = api_chain(handler, auth);
        let req = test_req(Method::GET, "/v1/models?api_key=secret-123");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn api_key_exempt_path_skips_auth() {
        let handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let auth = ApiKeyAuth::new("x-api-key")
            .require_for("/public", false)
            .with_key("secret-123", serde_json::json!({}));
        let chain = api_chain(handler, auth);
        // No key on an exempt path → 200.
        let req = test_req(Method::GET, "/public/health");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        // No key on a protected path → 401.
        let req = test_req(Method::GET, "/v1/models");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn api_key_claims_injected_to_extensions() {
        let seen = Arc::new(std::sync::Mutex::new(None::<serde_json::Value>));
        let seen2 = seen.clone();
        let handler: HandlerFn<TestBody> = Arc::new(move |req: Request<TestBody>| {
            let s = seen2.clone();
            Box::pin(async move {
                let claims = req.extensions().get::<serde_json::Value>().cloned();
                *s.lock().unwrap() = claims;
                Ok(json_response(StatusCode::OK, "ok"))
            })
        });
        let auth = ApiKeyAuth::new("x-api-key")
            .with_key("secret-123", serde_json::json!({"sub": "user-9"}));
        let chain = api_chain(handler, auth);
        let req = test_req_with_header(Method::GET, "/v1/models", "x-api-key", "secret-123");
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let claims = seen.lock().unwrap();
        assert_eq!(claims.as_ref().unwrap()["sub"], "user-9");
    }

    // --- OAuth2Password tests ---------------------------------------------

    fn form_req(method: Method, uri: &str, form_body: &str) -> Request<TestBody> {
        Request::builder()
            .method(method)
            .uri(uri)
            .header("content-type", "application/x-www-form-urlencoded")
            .body(TestBody::new(Bytes::from(form_body.to_string())))
            .unwrap()
    }

    #[tokio::test]
    async fn oauth2_non_post_returns_405() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|_, _| Some(serde_json::json!({"sub": "test"}))),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req = test_req(Method::GET, "/token");
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::METHOD_NOT_ALLOWED);
    }

    #[tokio::test]
    async fn oauth2_wrong_content_type_returns_415() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|_, _| Some(serde_json::json!({"sub": "test"}))),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req = Request::builder()
            .method(Method::POST)
            .uri("/token")
            .header("content-type", "application/json")
            .body(TestBody::new(Bytes::from(r#"{"a":1}"#)))
            .unwrap();
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNSUPPORTED_MEDIA_TYPE);
    }

    #[tokio::test]
    async fn oauth2_bad_grant_type_returns_400() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|_, _| Some(serde_json::json!({"sub": "test"}))),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req = form_req(
            Method::POST,
            "/token",
            "grant_type=client_credentials&username=alice&password=secret",
        );
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn oauth2_missing_credentials_returns_400() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|_, _| Some(serde_json::json!({"sub": "test"}))),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req = form_req(Method::POST, "/token", "grant_type=password");
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_REQUEST);
    }

    #[tokio::test]
    async fn oauth2_invalid_credentials_returns_401() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|u, p| {
                if u == "alice" && p == "correct" {
                    Some(serde_json::json!({"sub": "alice"}))
                } else {
                    None
                }
            }),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req =
            form_req(Method::POST, "/token", "grant_type=password&username=alice&password=wrong");
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::UNAUTHORIZED);
    }

    #[tokio::test]
    async fn oauth2_valid_credentials_returns_token() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|u, p| {
                if u == "alice" && p == "secret" {
                    Some(serde_json::json!({"sub": "alice", "roles": ["user"]}))
                } else {
                    None
                }
            }),
            jsonwebtoken::EncodingKey::from_secret(b"test"),
            jsonwebtoken::DecodingKey::from_secret(b"test"),
        );
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req =
            form_req(Method::POST, "/token", "grant_type=password&username=alice&password=secret");
        let resp = handler(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        let body: serde_json::Value = serde_json::from_slice(
            &http_body_util::BodyExt::collect(resp.into_body()).await.unwrap().to_bytes(),
        )
        .unwrap();
        assert!(body.get("access_token").and_then(|v| v.as_str()).is_some());
        assert_eq!(body["token_type"], "bearer");
        assert!(body["expires_in"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn oauth2_issued_token_verified_by_jwt_auth() {
        let oauth2 = OAuth2Password::new(
            Arc::new(|u, p| {
                if u == "alice" && p == "secret" {
                    Some(serde_json::json!({"sub": "alice", "roles": ["admin"]}))
                } else {
                    None
                }
            }),
            jsonwebtoken::EncodingKey::from_secret(b"test_key"),
            jsonwebtoken::DecodingKey::from_secret(b"test_key"),
        );
        // 1. Get a token from the token handler.
        let handler: HandlerFn<TestBody> = oauth2.token_handler();
        let req =
            form_req(Method::POST, "/token", "grant_type=password&username=alice&password=secret");
        let resp = handler(req).await.unwrap();
        let body: serde_json::Value = serde_json::from_slice(
            &http_body_util::BodyExt::collect(resp.into_body()).await.unwrap().to_bytes(),
        )
        .unwrap();
        let token = body["access_token"].as_str().unwrap().to_string();

        // 2. Create a JwtAuth from the OAuth2 provider and verify.
        let jwt_auth = oauth2.jwt_auth();
        let handler2: HandlerFn<TestBody> = Arc::new(|req: Request<TestBody>| {
            // The JwtAuth middleware injects claims into extensions
            let claims = req.extensions().get::<serde_json::Value>().cloned();
            Box::pin(async move {
                let body = serde_json::json!({
                    "claims": claims,
                    "ok": true,
                });
                Ok(json_response(StatusCode::OK, &serde_json::to_string(&body).unwrap()))
            })
        });
        let mut chain = MiddlewareChain::new(handler2);
        chain.add(jwt_auth);
        let req = test_req_with_header(
            Method::GET,
            "/protected",
            "authorization",
            &format!("Bearer {}", token),
        );
        let resp = chain.run(req).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }
}
