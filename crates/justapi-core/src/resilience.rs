use std::sync::Arc;
use std::time::Duration;

use anyhow::Result;
use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::body::Bytes;
use hyper::{Request, Response, StatusCode};
use tokio::sync::{Mutex, Semaphore};
use tokio::time::Instant;

use crate::middleware::{Middleware, Next};
use crate::ResponseBody;

// ---------------------------------------------------------------------------
// Circuit Breaker
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum BreakerState {
    Closed,
    Open(Instant),
    HalfOpen,
}

struct BreakerInner {
    state: BreakerState,
    failure_count: u32,
    success_count: u32,
    half_open_allowed: u32,
    half_open_used: u32,
}

#[derive(Debug, Clone)]
pub struct CircuitBreakerConfig {
    pub failure_threshold: u32,
    pub success_threshold: u32,
    pub open_timeout: Duration,
    pub half_open_max_requests: u32,
}

impl Default for CircuitBreakerConfig {
    fn default() -> Self {
        Self {
            failure_threshold: 5,
            success_threshold: 3,
            open_timeout: Duration::from_secs(30),
            half_open_max_requests: 3,
        }
    }
}

#[derive(Clone)]
pub struct CircuitBreaker {
    inner: Arc<Mutex<BreakerInner>>,
    config: CircuitBreakerConfig,
}

impl CircuitBreaker {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            inner: Arc::new(Mutex::new(BreakerInner {
                state: BreakerState::Closed,
                failure_count: 0,
                success_count: 0,
                half_open_allowed: config.half_open_max_requests,
                half_open_used: 0,
            })),
            config,
        }
    }

    async fn try_acquire(&self) -> bool {
        let mut inner = self.inner.lock().await;
        match &inner.state {
            BreakerState::Closed => true,
            BreakerState::Open(opened_at) => {
                if opened_at.elapsed() >= self.config.open_timeout {
                    inner.state = BreakerState::HalfOpen;
                    inner.half_open_used = 0;
                    inner.success_count = 0;
                    true
                } else {
                    false
                }
            }
            BreakerState::HalfOpen => {
                if inner.half_open_used < inner.half_open_allowed {
                    inner.half_open_used += 1;
                    true
                } else {
                    false
                }
            }
        }
    }

    async fn record_success(&self) {
        let mut inner = self.inner.lock().await;
        match &inner.state {
            BreakerState::HalfOpen => {
                inner.success_count += 1;
                if inner.success_count >= self.config.success_threshold {
                    inner.state = BreakerState::Closed;
                    inner.failure_count = 0;
                    inner.success_count = 0;
                }
            }
            BreakerState::Closed => {
                inner.failure_count = 0;
            }
            _ => {}
        }
    }

    async fn record_failure(&self) {
        let mut inner = self.inner.lock().await;
        match &inner.state {
            BreakerState::Closed => {
                inner.failure_count += 1;
                if inner.failure_count >= self.config.failure_threshold {
                    inner.state = BreakerState::Open(Instant::now());
                    inner.failure_count = 0;
                }
            }
            BreakerState::HalfOpen => {
                inner.state = BreakerState::Open(Instant::now());
                inner.failure_count = 0;
            }
            _ => {}
        }
    }
}

pub struct CircuitBreakerMiddleware {
    breaker: CircuitBreaker,
}

impl CircuitBreakerMiddleware {
    pub fn new(breaker: CircuitBreaker) -> Self {
        Self { breaker }
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for CircuitBreakerMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        if !self.breaker.try_acquire().await {
            return Ok(circuit_open_response());
        }

        match next.run(req).await {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    self.breaker.record_failure().await;
                } else {
                    self.breaker.record_success().await;
                }
                Ok(resp)
            }
            Err(e) => {
                self.breaker.record_failure().await;
                Err(e)
            }
        }
    }
}

pub struct PerRouteCircuitBreakerMiddleware {
    config: CircuitBreakerConfig,
    breakers: std::sync::RwLock<std::collections::HashMap<String, CircuitBreaker>>,
}

impl PerRouteCircuitBreakerMiddleware {
    pub fn new(config: CircuitBreakerConfig) -> Self {
        Self {
            config,
            breakers: std::sync::RwLock::new(std::collections::HashMap::new()),
        }
    }

    fn get_breaker(&self, path: &str) -> CircuitBreaker {
        {
            let read = self.breakers.read().unwrap();
            if let Some(cb) = read.get(path) {
                return cb.clone();
            }
        }
        let mut write = self.breakers.write().unwrap();
        // check again to prevent race conditions
        if let Some(cb) = write.get(path) {
            return cb.clone();
        }
        let cb = CircuitBreaker::new(self.config.clone());
        write.insert(path.to_string(), cb.clone());
        cb
    }
}

#[async_trait]
impl<B: Send + Sync + 'static> Middleware<B> for PerRouteCircuitBreakerMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let path = req.uri().path().to_string();
        let breaker = self.get_breaker(&path);

        if !breaker.try_acquire().await {
            return Ok(circuit_open_response());
        }

        match next.run(req).await {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    breaker.record_failure().await;
                } else {
                    breaker.record_success().await;
                }
                Ok(resp)
            }
            Err(e) => {
                breaker.record_failure().await;
                Err(e)
            }
        }
    }
}

fn circuit_open_response() -> Response<ResponseBody> {
    let body = r#"{"error":"service unavailable — circuit breaker open"}"#;
    let mut resp = json_text_response(StatusCode::SERVICE_UNAVAILABLE, body);
    resp.headers_mut()
        .insert("retry-after", "30".parse().unwrap());
    resp
}

// ---------------------------------------------------------------------------
// Timeout Middleware
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct TimeoutMiddleware {
    duration: Duration,
}

impl TimeoutMiddleware {
    pub fn new(duration: Duration) -> Self {
        Self { duration }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for TimeoutMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        match tokio::time::timeout(self.duration, next.run(req)).await {
            Ok(result) => result,
            Err(_) => Ok(timeout_response(self.duration)),
        }
    }
}

fn timeout_response(timeout: Duration) -> Response<ResponseBody> {
    let body = format!(
        r#"{{"error":"request timed out after {}ms"}}"#,
        timeout.as_millis()
    );
    json_text_response(StatusCode::GATEWAY_TIMEOUT, &body)
}

// ---------------------------------------------------------------------------
// Retry with Backoff
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct RetryPolicy {
    pub max_retries: u32,
    pub base_delay: Duration,
    pub max_delay: Duration,
    pub jitter: bool,
}

impl Default for RetryPolicy {
    fn default() -> Self {
        Self {
            max_retries: 3,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: true,
        }
    }
}

impl RetryPolicy {
    pub fn delay(&self, attempt: u32) -> Duration {
        let exp = 2u32.saturating_pow(attempt);
        let base_ms = self.base_delay.as_millis() as u64;
        let delay_ms = base_ms.saturating_mul(exp as u64);
        let capped = delay_ms.min(self.max_delay.as_millis() as u64);
        if self.jitter {
            let jitter_range = capped / 5;
            let jitter = if jitter_range > 0 {
                rand::random::<u64>() % jitter_range
            } else {
                0
            };
            Duration::from_millis(capped + jitter)
        } else {
            Duration::from_millis(capped)
        }
    }

    pub fn retryable(&self, status: StatusCode) -> bool {
        status.is_server_error()
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------
// Bulkhead
// ---------------------------------------------------------------------------

#[derive(Clone)]
pub struct BulkheadConfig {
    pub max_concurrent: u32,
    pub max_wait: Duration,
}

impl Default for BulkheadConfig {
    fn default() -> Self {
        Self {
            max_concurrent: 10,
            max_wait: Duration::from_secs(1),
        }
    }
}

#[derive(Clone)]
pub struct Bulkhead {
    semaphore: Arc<Semaphore>,
    config: BulkheadConfig,
}

impl Bulkhead {
    pub fn new(config: BulkheadConfig) -> Self {
        Self {
            semaphore: Arc::new(Semaphore::new(config.max_concurrent as usize)),
            config,
        }
    }
}

pub struct BulkheadMiddleware {
    bulkhead: Bulkhead,
}

impl BulkheadMiddleware {
    pub fn new(bulkhead: Bulkhead) -> Self {
        Self { bulkhead }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for BulkheadMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let permit = tokio::time::timeout(
            self.bulkhead.config.max_wait,
            self.bulkhead.semaphore.acquire(),
        )
        .await;

        match permit {
            Ok(Ok(permit)) => {
                let result = next.run(req).await;
                drop(permit);
                result
            }
            _ => Ok(json_text_response(
                StatusCode::SERVICE_UNAVAILABLE,
                r#"{"error":"too many concurrent requests — bulkhead limit reached"}"#,
            )),
        }
    }
}

// ---------------------------------------------------------------------------
// Fallback Middleware
// ---------------------------------------------------------------------------

use std::collections::HashMap;

#[derive(Clone)]
pub struct FallbackPolicy {
    fallbacks: Arc<HashMap<String, FallbackEntry>>,
    default_fallback: Option<FallbackEntry>,
}

#[derive(Clone)]
pub struct FallbackEntry {
    pub status: StatusCode,
    pub body: String,
    pub content_type: String,
}

impl FallbackPolicy {
    pub fn new() -> Self {
        Self {
            fallbacks: Arc::new(HashMap::new()),
            default_fallback: None,
        }
    }

    pub fn add(mut self, path_prefix: &str, entry: FallbackEntry) -> Self {
        Arc::make_mut(&mut self.fallbacks).insert(path_prefix.to_string(), entry);
        self
    }

    pub fn with_default(mut self, entry: FallbackEntry) -> Self {
        self.default_fallback = Some(entry);
        self
    }

    fn get(&self, path: &str) -> Option<&FallbackEntry> {
        for (prefix, entry) in self.fallbacks.iter() {
            if path.starts_with(prefix) {
                return Some(entry);
            }
        }
        self.default_fallback.as_ref()
    }
}

impl Default for FallbackPolicy {
    fn default() -> Self {
        Self::new()
    }
}

pub struct FallbackMiddleware {
    policy: FallbackPolicy,
}

impl FallbackMiddleware {
    pub fn new(policy: FallbackPolicy) -> Self {
        Self { policy }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for FallbackMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let path = req.uri().path().to_string();

        match next.run(req).await {
            Ok(resp) => {
                if resp.status().is_server_error() {
                    if let Some(fb) = self.policy.get(&path) {
                        return Ok(fallback_response(fb));
                    }
                }
                Ok(resp)
            }
            Err(_e) => {
                if let Some(fb) = self.policy.get(&path) {
                    Ok(fallback_response(fb))
                } else {
                    Ok(json_text_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"error":"internal server error"}"#,
                    ))
                }
            }
        }
    }
}

fn fallback_response(fb: &FallbackEntry) -> Response<ResponseBody> {
    let body = fb.body.clone();
    let content_type = fb.content_type.clone();
    Response::builder()
        .status(fb.status)
        .header("content-type", content_type)
        .header("content-length", body.len().to_string())
        .body(http_body_util::combinators::UnsyncBoxBody::new(
            http_body_util::Full::new(Bytes::from(body))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Chaos Middleware (testing only — injects failures)
// ---------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct ChaosConfig {
    pub enabled: bool,
    pub latency_p: f64,
    pub latency_min_ms: u64,
    pub latency_max_ms: u64,
    pub error_p: f64,
    pub error_status: StatusCode,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            latency_p: 0.0,
            latency_min_ms: 100,
            latency_max_ms: 500,
            error_p: 0.0,
            error_status: StatusCode::INTERNAL_SERVER_ERROR,
        }
    }
}

pub struct ChaosMiddleware {
    config: ChaosConfig,
}

impl ChaosMiddleware {
    pub fn new(config: ChaosConfig) -> Self {
        Self { config }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for ChaosMiddleware {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        if !self.config.enabled {
            return next.run(req).await;
        }

        if self.config.error_p > 0.0 {
            let roll: f64 = rand::random();
            if roll < self.config.error_p {
                let body = format!(
                    r#"{{"error":"chaos injected failure","status":{}}}"#,
                    self.config.error_status.as_u16()
                );
                return Ok(json_text_response(self.config.error_status, &body));
            }
        }

        if self.config.latency_p > 0.0 {
            let roll: f64 = rand::random();
            if roll < self.config.latency_p {
                let delay_ms = self.config.latency_min_ms
                    + (rand::random::<u64>()
                        % (self.config.latency_max_ms - self.config.latency_min_ms + 1));
                tokio::time::sleep(Duration::from_millis(delay_ms)).await;
            }
        }

        next.run(req).await
    }
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn json_text_response(status: StatusCode, body: &str) -> Response<ResponseBody> {
    Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(http_body_util::combinators::UnsyncBoxBody::new(
            http_body_util::Full::new(Bytes::from(body.to_string()))
                .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
        ))
        .unwrap()
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_response;
    use crate::middleware::HandlerFn;
    use std::sync::Arc;

    type TestBody = http_body_util::Full<Bytes>;

    fn ok_handler() -> HandlerFn<TestBody> {
        Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, r#"{"ok":true}"#)) })
        })
    }

    fn error_handler() -> HandlerFn<TestBody> {
        Arc::new(|_req: Request<TestBody>| {
            Box::pin(async { Err(anyhow::anyhow!("handler error")) })
        })
    }

    fn five_hundred_handler() -> HandlerFn<TestBody> {
        Arc::new(|_req: Request<TestBody>| {
            Box::pin(async {
                Ok(json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    r#"{"error":"server error"}"#,
                ))
            })
        })
    }

    fn test_req(method: hyper::Method, uri: &str) -> Request<TestBody> {
        Request::builder()
            .method(method)
            .uri(uri)
            .body(TestBody::new(Bytes::new()))
            .unwrap()
    }

    // -----------------------------------------------------------------------
    // Circuit Breaker tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_circuit_breaker_passes_healthy_requests() {
        let breaker = CircuitBreaker::new(CircuitBreakerConfig::default());
        let mw = CircuitBreakerMiddleware::new(breaker);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_circuit_breaker_opens_on_too_many_failures() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 2,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 2,
        };
        let breaker = CircuitBreaker::new(config);
        let mw = CircuitBreakerMiddleware::new(breaker.clone());
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);

        // 3 failures to open
        for _ in 0..3 {
            let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
            assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        }

        // Now circuit is open, should get 503
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_circuit_breaker_recovery() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 2,
            open_timeout: Duration::from_millis(50),
            half_open_max_requests: 3,
        };
        let breaker = CircuitBreaker::new(config);

        // We need a setup where we can flip between failing and passing.
        // Use a shared failure flag.
        let failing = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let failing_clone = failing.clone();

        let dynamic_handler: HandlerFn<TestBody> = Arc::new(move |_req: Request<TestBody>| {
            let f = failing_clone.clone();
            Box::pin(async move {
                if f.load(std::sync::atomic::Ordering::Relaxed) {
                    Ok(json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"error":"server error"}"#,
                    ))
                } else {
                    Ok(json_response(StatusCode::OK, r#"{"ok":true}"#))
                }
            })
        });

        let mut chain = crate::middleware::MiddlewareChain::new(dynamic_handler);
        chain.add(CircuitBreakerMiddleware::new(breaker.clone()));

        // 2 failures to open
        for _ in 0..2 {
            chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        }

        // Should be open
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        // Wait for timeout
        tokio::time::sleep(Duration::from_millis(60)).await;

        // Handler now succeeds
        failing.store(false, std::sync::atomic::Ordering::Relaxed);

        // Should be half-open now, allow request
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Second success to close
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // Now circuit is closed (success threshold reached)
        // If we fail again, it should reset failure count (closed state resets on success)
        failing.store(true, std::sync::atomic::Ordering::Relaxed);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // One more failure (first one should have reset failure count to 1)
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // Should have hit threshold again (2 failures in a row)
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_circuit_breaker_handles_errors() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 1,
        };
        let breaker = CircuitBreaker::new(config);
        let mw = CircuitBreakerMiddleware::new(breaker.clone());
        let mut chain = crate::middleware::MiddlewareChain::new(error_handler());
        chain.add(mw);

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await;
        assert!(resp.is_err());

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await;
        assert!(resp.is_err());

        // Circuit open — should get 503 even though handler would return Err
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // -----------------------------------------------------------------------
    // Timeout tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_timeout_passes_fast_requests() {
        let mw = TimeoutMiddleware::new(Duration::from_secs(5));
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_timeout_returns_504() {
        let slow_handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(json_response(StatusCode::OK, r#"{"ok":true}"#))
            })
        });

        let mw = TimeoutMiddleware::new(Duration::from_millis(10));
        let mut chain = crate::middleware::MiddlewareChain::new(slow_handler);
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    // -----------------------------------------------------------------------
    // Retry policy tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_retry_policy_delay() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(10),
            max_delay: Duration::from_secs(10),
            jitter: false,
        };
        assert_eq!(policy.delay(0).as_millis(), 10);
        assert_eq!(policy.delay(1).as_millis(), 20);
        assert_eq!(policy.delay(2).as_millis(), 40);
    }

    #[tokio::test]
    async fn test_retry_policy_retryable() {
        let policy = RetryPolicy::default();
        assert!(policy.retryable(StatusCode::INTERNAL_SERVER_ERROR));
        assert!(policy.retryable(StatusCode::BAD_GATEWAY));
        assert!(!policy.retryable(StatusCode::BAD_REQUEST));
        assert!(!policy.retryable(StatusCode::OK));
    }

    #[tokio::test]
    async fn test_retry_policy_delay_with_jitter() {
        let policy = RetryPolicy {
            max_retries: 2,
            base_delay: Duration::from_millis(100),
            max_delay: Duration::from_secs(10),
            jitter: true,
        };
        // jitter adds 0-20% of delay, so delay should be >= base and <= base*1.2
        let d = policy.delay(0);
        assert!(d >= Duration::from_millis(100));
        assert!(d <= Duration::from_millis(120));
    }

    // -----------------------------------------------------------------------
    // Bulkhead tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_bulkhead_passes_normal_requests() {
        let config = BulkheadConfig {
            max_concurrent: 10,
            max_wait: Duration::from_secs(1),
        };
        let bulkhead = Bulkhead::new(config);
        let mw = BulkheadMiddleware::new(bulkhead);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_bulkhead_rejects_when_full() {
        let config = BulkheadConfig {
            max_concurrent: 1,
            max_wait: Duration::from_millis(50),
        };
        let bulkhead = Bulkhead::new(config);
        let mw = BulkheadMiddleware::new(bulkhead);

        let slow_handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(200)).await;
                Ok(json_response(StatusCode::OK, r#"{"ok":true}"#))
            })
        });

        let mut chain = crate::middleware::MiddlewareChain::new(slow_handler);
        chain.add(mw);

        let chain1 = chain.clone();
        let req1 = test_req(hyper::Method::GET, "/");
        let h1 = tokio::spawn(async move { chain1.run(req1).await });

        tokio::time::sleep(Duration::from_millis(20)).await;

        let req2 = test_req(hyper::Method::GET, "/");
        let resp = chain.run(req2).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        h1.await.unwrap().unwrap();
    }

    // -----------------------------------------------------------------------
    // Fallback tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fallback_passes_successful_response() {
        let policy = FallbackPolicy::new().add(
            "/",
            FallbackEntry {
                status: StatusCode::OK,
                body: r#"{"cached":true}"#.to_string(),
                content_type: "application/json".to_string(),
            },
        );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fallback_serves_on_5xx() {
        let policy = FallbackPolicy::new().add(
            "/",
            FallbackEntry {
                status: StatusCode::OK,
                body: r#"{"cached":true}"#.to_string(),
                content_type: "application/json".to_string(),
            },
        );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fallback_serves_on_handler_error() {
        let policy = FallbackPolicy::new().add(
            "/",
            FallbackEntry {
                status: StatusCode::OK,
                body: r#"{"cached":true}"#.to_string(),
                content_type: "application/json".to_string(),
            },
        );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(error_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fallback_path_matching() {
        let policy = FallbackPolicy::new()
            .add(
                "/api",
                FallbackEntry {
                    status: StatusCode::OK,
                    body: r#"{"cached":"api"}"#.to_string(),
                    content_type: "application/json".to_string(),
                },
            )
            .add(
                "/users",
                FallbackEntry {
                    status: StatusCode::OK,
                    body: r#"{"cached":"users"}"#.to_string(),
                    content_type: "application/json".to_string(),
                },
            );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);

        let resp = chain
            .run(test_req(hyper::Method::GET, "/api/data"))
            .await
            .unwrap();
        assert!(
            String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
                .contains(r#""api"#)
        );

        let resp = chain
            .run(test_req(hyper::Method::GET, "/users/123"))
            .await
            .unwrap();
        assert!(
            String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
                .contains(r#""users"#)
        );
    }

    #[tokio::test]
    async fn test_fallback_no_match_returns_original() {
        let policy = FallbackPolicy::default();
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);
        let resp = chain
            .run(test_req(hyper::Method::GET, "/no-fallback"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_fallback_default_on_error() {
        let policy = FallbackPolicy::new().with_default(FallbackEntry {
            status: StatusCode::OK,
            body: r#"{"cached":"default"}"#.to_string(),
            content_type: "application/json".to_string(),
        });
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(error_handler());
        chain.add(mw);
        let resp = chain
            .run(test_req(hyper::Method::GET, "/anything"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // Chaos middleware tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_chaos_disabled_passes_through() {
        let config = ChaosConfig {
            enabled: false,
            error_p: 1.0,
            ..Default::default()
        };
        let mw = ChaosMiddleware::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chaos_injects_errors() {
        let config = ChaosConfig {
            enabled: true,
            error_p: 1.0,
            error_status: StatusCode::INTERNAL_SERVER_ERROR,
            ..Default::default()
        };
        let mw = ChaosMiddleware::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_chaos_injects_latency() {
        let config = ChaosConfig {
            enabled: true,
            latency_p: 1.0,
            latency_min_ms: 20,
            latency_max_ms: 30,
            ..Default::default()
        };
        let mw = ChaosMiddleware::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let start = std::time::Instant::now();
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        let elapsed = start.elapsed();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(elapsed >= Duration::from_millis(20));
    }

    // -----------------------------------------------------------------------
    // Combined resilience tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_timeout_and_bulkhead_together() {
        let timeout_mw = TimeoutMiddleware::new(Duration::from_secs(5));
        let bulkhead = Bulkhead::new(BulkheadConfig {
            max_concurrent: 5,
            ..Default::default()
        });
        let bulkhead_mw = BulkheadMiddleware::new(bulkhead);

        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(timeout_mw);
        chain.add(bulkhead_mw);

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_circuit_breaker_and_fallback_together() {
        let config = CircuitBreakerConfig {
            failure_threshold: 2,
            success_threshold: 1,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 1,
        };
        let breaker = CircuitBreaker::new(config);
        let cb_mw = CircuitBreakerMiddleware::new(breaker);

        let fallback = FallbackPolicy::new().with_default(FallbackEntry {
            status: StatusCode::OK,
            body: r#"{"cached":"fallback"}"#.to_string(),
            content_type: "application/json".to_string(),
        });
        let fb_mw = FallbackMiddleware::new(fallback);

        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(fb_mw);
        chain.add(cb_mw);

        for _ in 0..2 {
            let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
            assert_eq!(resp.status(), StatusCode::OK);
        }

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert!(
            String::from_utf8_lossy(&resp.into_body().collect().await.unwrap().to_bytes())
                .contains(r#""fallback"#)
        );
    }

    // -----------------------------------------------------------------------
    // Additional Circuit Breaker edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_circuit_breaker_half_open_failure_reopens() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 1,
            open_timeout: Duration::from_millis(20),
            half_open_max_requests: 1,
        };
        let breaker = CircuitBreaker::new(config);

        // Force open
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(CircuitBreakerMiddleware::new(breaker.clone()));
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // Now open
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(25)).await;

        // In half-open, handler still fails → goes back to open
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Should be open again
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_circuit_breaker_half_open_limited_probe() {
        let config = CircuitBreakerConfig {
            failure_threshold: 1,
            success_threshold: 5,
            open_timeout: Duration::from_millis(20),
            half_open_max_requests: 2,
        };
        let breaker = CircuitBreaker::new(config);

        // Dynamic handler: fails on first call, succeeds on subsequent
        let call_count = Arc::new(std::sync::atomic::AtomicU32::new(0));
        let cc = call_count.clone();
        let handler: HandlerFn<TestBody> = Arc::new(move |_req: Request<TestBody>| {
            let c = cc.clone();
            Box::pin(async move {
                let n = c.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
                if n == 0 {
                    Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, "err"))
                } else {
                    Ok(json_response(StatusCode::OK, "ok"))
                }
            })
        });
        let mut chain = crate::middleware::MiddlewareChain::new(handler);
        chain.add(CircuitBreakerMiddleware::new(breaker.clone()));

        // First call: failure → opens breaker
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // Already open → 503
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);

        // Wait for half-open
        tokio::time::sleep(Duration::from_millis(25)).await;

        // 1st request transitions Open→HalfOpen (doesn't count against limit)
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 2nd request: HalfOpen, half_open_used=1
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 3rd request: HalfOpen, half_open_used=2
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // 4th request: half_open_used=2 >= half_open_allowed=2 → blocked
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_circuit_breaker_zero_threshold() {
        let config = CircuitBreakerConfig {
            failure_threshold: 0,
            success_threshold: 1,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 1,
        };
        let breaker = CircuitBreaker::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(CircuitBreakerMiddleware::new(breaker));
        // With 0 threshold, even a single failure may cause unexpected behavior.
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
    }

    #[tokio::test]
    async fn test_circuit_breaker_success_resets_failure_count() {
        let config = CircuitBreakerConfig {
            failure_threshold: 3,
            success_threshold: 1,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 1,
        };
        let breaker = CircuitBreaker::new(config);
        let failing = Arc::new(std::sync::atomic::AtomicBool::new(true));
        let f = failing.clone();
        let handler: HandlerFn<TestBody> = Arc::new(move |req: Request<TestBody>| {
            let _ = req;
            let g = f.clone();
            Box::pin(async move {
                if g.load(std::sync::atomic::Ordering::Relaxed) {
                    Ok(json_response(StatusCode::INTERNAL_SERVER_ERROR, "err"))
                } else {
                    Ok(json_response(StatusCode::OK, "ok"))
                }
            })
        });
        let mut chain = crate::middleware::MiddlewareChain::new(handler);
        chain.add(CircuitBreakerMiddleware::new(breaker.clone()));

        // 2 failures
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);

        // Success resets failure count
        failing.store(false, std::sync::atomic::Ordering::Relaxed);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);

        // More failures should need 3 again (failure count was reset)
        failing.store(true, std::sync::atomic::Ordering::Relaxed);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        // Now open
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    // -----------------------------------------------------------------------
    // Additional Timeout edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_timeout_zero_duration() {
        let slow_handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_millis(5)).await;
                Ok(json_response(StatusCode::OK, "ok"))
            })
        });
        let mw = TimeoutMiddleware::new(Duration::from_millis(0));
        let mut chain = crate::middleware::MiddlewareChain::new(slow_handler);
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::GATEWAY_TIMEOUT);
    }

    #[tokio::test]
    async fn test_timeout_very_long() {
        let mw = TimeoutMiddleware::new(Duration::from_secs(60));
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    // -----------------------------------------------------------------------
    // Additional Retry Policy edge case tests
    // -----------------------------------------------------------------------

    #[test]
    fn test_retry_policy_max_delay_cap() {
        let policy = RetryPolicy {
            max_retries: 10,
            base_delay: Duration::from_secs(10),
            max_delay: Duration::from_secs(5),
            jitter: false,
        };
        // Even at attempt 0, delay is capped at max_delay
        assert_eq!(policy.delay(0), Duration::from_secs(5));
        assert_eq!(policy.delay(10), Duration::from_secs(5));
    }

    #[test]
    fn test_retry_policy_zero_base_delay() {
        let policy = RetryPolicy {
            max_retries: 3,
            base_delay: Duration::from_millis(0),
            max_delay: Duration::from_secs(10),
            jitter: false,
        };
        assert_eq!(policy.delay(0), Duration::from_millis(0));
        assert_eq!(policy.delay(1), Duration::from_millis(0));
    }

    #[test]
    fn test_retry_policy_non_retryable_statuses() {
        let policy = RetryPolicy::default();
        assert!(!policy.retryable(StatusCode::CONTINUE));
        assert!(!policy.retryable(StatusCode::SWITCHING_PROTOCOLS));
        assert!(!policy.retryable(StatusCode::FOUND));
        assert!(!policy.retryable(StatusCode::NOT_MODIFIED));
        assert!(!policy.retryable(StatusCode::TEMPORARY_REDIRECT));
        assert!(!policy.retryable(StatusCode::PERMANENT_REDIRECT));
        assert!(!policy.retryable(StatusCode::TOO_MANY_REQUESTS));
        assert!(!policy.retryable(StatusCode::REQUEST_TIMEOUT));
        assert!(policy.retryable(StatusCode::SERVICE_UNAVAILABLE));
        assert!(policy.retryable(StatusCode::BAD_GATEWAY));
        assert!(policy.retryable(StatusCode::GATEWAY_TIMEOUT));
    }

    // -----------------------------------------------------------------------
    // Additional Bulkhead edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_bulkhead_zero_concurrent() {
        let config = BulkheadConfig {
            max_concurrent: 0,
            max_wait: Duration::from_millis(10),
        };
        let bulkhead = Bulkhead::new(config);
        let mw = BulkheadMiddleware::new(bulkhead);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
    }

    #[tokio::test]
    async fn test_bulkhead_release_on_error() {
        let config = BulkheadConfig {
            max_concurrent: 1,
            max_wait: Duration::from_millis(50),
        };
        let bulkhead = Bulkhead::new(config);
        let mw = BulkheadMiddleware::new(bulkhead);

        let error_handler: HandlerFn<TestBody> =
            Arc::new(|_req: Request<TestBody>| Box::pin(async { Err(anyhow::anyhow!("oops")) }));
        let mut chain = crate::middleware::MiddlewareChain::new(error_handler);
        chain.add(mw);

        // First request fails, permit is released on drop
        let chain1 = chain.clone();
        let h1 = tokio::spawn(async move { chain1.run(test_req(hyper::Method::GET, "/")).await });
        tokio::time::sleep(Duration::from_millis(20)).await;
        // Second request — permit should have been released
        let result = chain.run(test_req(hyper::Method::GET, "/")).await;
        assert!(result.is_err() || result.unwrap().status().is_success());
        // First request returned Err
        assert!(h1.await.unwrap().is_err());
    }

    // -----------------------------------------------------------------------
    // Additional Fallback edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_fallback_empty_path_prefix() {
        let policy = FallbackPolicy::new().add(
            "",
            FallbackEntry {
                status: StatusCode::OK,
                body: r#"{"cached":"root"}"#.to_string(),
                content_type: "application/json".to_string(),
            },
        );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);
        let resp = chain
            .run(test_req(hyper::Method::GET, "/anything"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_fallback_content_type_preserved() {
        let policy = FallbackPolicy::new().add(
            "/api",
            FallbackEntry {
                status: StatusCode::OK,
                body: "<cached>data</cached>".to_string(),
                content_type: "application/xml".to_string(),
            },
        );
        let mw = FallbackMiddleware::new(policy);
        let mut chain = crate::middleware::MiddlewareChain::new(five_hundred_handler());
        chain.add(mw);
        let resp = chain
            .run(test_req(hyper::Method::GET, "/api/data"))
            .await
            .unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
        assert_eq!(resp.headers()["content-type"], "application/xml");
        let body = resp.into_body().collect().await.unwrap().to_bytes();
        assert_eq!(&body[..], b"<cached>data</cached>");
    }

    // -----------------------------------------------------------------------
    // Additional Chaos edge case tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_chaos_zero_probability_with_enabled() {
        let config = ChaosConfig {
            enabled: true,
            error_p: 0.0,
            latency_p: 0.0,
            latency_min_ms: 100,
            latency_max_ms: 200,
            error_status: StatusCode::INTERNAL_SERVER_ERROR,
        };
        let mw = ChaosMiddleware::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chaos_custom_error_status() {
        let config = ChaosConfig {
            enabled: true,
            error_p: 1.0,
            error_status: StatusCode::BAD_GATEWAY,
            ..Default::default()
        };
        let mw = ChaosMiddleware::new(config);
        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(mw);
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::BAD_GATEWAY);
    }

    // -----------------------------------------------------------------------
    // Additional combined resilience tests
    // -----------------------------------------------------------------------

    #[tokio::test]
    async fn test_timeout_bulkhead_circuit_breaker_combined() {
        let timeout_mw = TimeoutMiddleware::new(Duration::from_secs(5));
        let bulkhead = Bulkhead::new(BulkheadConfig {
            max_concurrent: 2,
            max_wait: Duration::from_secs(1),
        });
        let bulkhead_mw = BulkheadMiddleware::new(bulkhead);
        let breaker = CircuitBreaker::new(CircuitBreakerConfig {
            failure_threshold: 5,
            success_threshold: 1,
            open_timeout: Duration::from_secs(60),
            half_open_max_requests: 3,
        });
        let cb_mw = CircuitBreakerMiddleware::new(breaker);

        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(timeout_mw);
        chain.add(bulkhead_mw);
        chain.add(cb_mw);

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_chaos_fallback_combined() {
        // Chaos injects error, fallback catches it
        let chaos = ChaosConfig {
            enabled: true,
            error_p: 1.0,
            error_status: StatusCode::INTERNAL_SERVER_ERROR,
            ..Default::default()
        };
        let chaos_mw = ChaosMiddleware::new(chaos);
        let policy = FallbackPolicy::new().add(
            "/",
            FallbackEntry {
                status: StatusCode::OK,
                body: r#"{"cached":"survived"}"#.to_string(),
                content_type: "application/json".to_string(),
            },
        );
        let fb_mw = FallbackMiddleware::new(policy);

        let mut chain = crate::middleware::MiddlewareChain::new(ok_handler());
        chain.add(fb_mw);
        chain.add(chaos_mw);

        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[tokio::test]
    async fn test_bulkhead_timeout_interaction() {
        // A slow handler that would be blocked by bulkhead, but timeout should fire first
        let config = BulkheadConfig {
            max_concurrent: 1,
            max_wait: Duration::from_millis(100),
        };
        let bulkhead = Bulkhead::new(config);
        let bulkhead_mw = BulkheadMiddleware::new(bulkhead);
        let timeout_mw = TimeoutMiddleware::new(Duration::from_millis(200));

        let slow_handler: HandlerFn<TestBody> = Arc::new(|_req: Request<TestBody>| {
            Box::pin(async {
                tokio::time::sleep(Duration::from_secs(10)).await;
                Ok(json_response(StatusCode::OK, "ok"))
            })
        });

        let mut chain = crate::middleware::MiddlewareChain::new(slow_handler);
        chain.add(bulkhead_mw);
        chain.add(timeout_mw);

        let chain1 = chain.clone();
        let h1 = tokio::spawn(async move { chain1.run(test_req(hyper::Method::GET, "/")).await });
        tokio::time::sleep(Duration::from_millis(20)).await;

        // This request should get 503 from bulkhead (not 504 from timeout)
        let resp = chain.run(test_req(hyper::Method::GET, "/")).await.unwrap();
        assert_eq!(resp.status(), StatusCode::SERVICE_UNAVAILABLE);
        h1.await.unwrap().unwrap();
    }

    // -----------------------------------------------------------------------
    // Circuit breaker config defaults test
    // -----------------------------------------------------------------------

    #[test]
    fn test_circuit_breaker_config_defaults() {
        let config = CircuitBreakerConfig::default();
        assert_eq!(config.failure_threshold, 5);
        assert_eq!(config.success_threshold, 3);
        assert_eq!(config.open_timeout, Duration::from_secs(30));
        assert_eq!(config.half_open_max_requests, 3);
    }

    #[test]
    fn test_bulkhead_config_defaults() {
        let config = BulkheadConfig::default();
        assert_eq!(config.max_concurrent, 10);
        assert_eq!(config.max_wait, Duration::from_secs(1));
    }

    #[test]
    fn test_chaos_config_defaults() {
        let config = ChaosConfig::default();
        assert!(!config.enabled);
    }

    #[test]
    fn test_retry_policy_defaults() {
        let policy = RetryPolicy::default();
        assert_eq!(policy.max_retries, 3);
        assert_eq!(policy.base_delay, Duration::from_millis(100));
        assert_eq!(policy.max_delay, Duration::from_secs(10));
        assert!(policy.jitter);
    }
}
