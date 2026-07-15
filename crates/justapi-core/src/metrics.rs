use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::sync::Mutex;
use std::time::Instant;

use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::body::Bytes;
use hyper::{Response, StatusCode};

use crate::ResponseBody;

/// Prometheus-compatible latency bucket boundaries in milliseconds.
const LATENCY_BUCKETS_MS: &[f64] =
    &[1.0, 2.5, 5.0, 10.0, 25.0, 50.0, 100.0, 250.0, 500.0, 1000.0, 2500.0, 5000.0, 10000.0];

/// A snapshot of all metric values at a point in time (for testing & Prometheus).
#[derive(Debug, Clone)]
pub struct MetricsSnapshot {
    pub requests_total: u64,
    pub errors_total: u64,
    pub bytes_in: u64,
    pub bytes_out: u64,
    pub active_connections: u64,
    pub status_2xx: u64,
    pub status_3xx: u64,
    pub status_4xx: u64,
    pub status_5xx: u64,
    pub latency_count: u64,
    pub latency_sum_ms: f64,
    /// Cumulative count per bucket (length = buckets.len() + 1; last is +Inf).
    pub latency_buckets: Vec<u64>,
}

/// A provider of extra Prometheus-formatted metric lines, appended to the
/// output of [`Metrics::prometheus`].  Used by the scheduler to expose prefix-
/// cache statistics without coupling `Metrics` to the inference crate.
pub trait MetricProvider: Send + Sync {
    fn render(&self) -> String;
}

impl<F> MetricProvider for F
where
    F: Fn() -> String + Send + Sync,
{
    fn render(&self) -> String {
        self()
    }
}

/// Shared metrics collector with latency histograms and status-code tracking.
#[derive(Clone)]
pub struct Metrics {
    inner: Arc<MetricsInner>,
}

struct LatencyHist {
    buckets: Vec<AtomicU64>,
    count: AtomicU64,
    sum_ms: AtomicU64,
}

impl LatencyHist {
    fn new() -> Self {
        let buckets = LATENCY_BUCKETS_MS.iter().map(|_| AtomicU64::new(0)).collect();
        Self { buckets, count: AtomicU64::new(0), sum_ms: AtomicU64::new(0) }
    }

    fn record(&self, latency_ms: f64) {
        self.count.fetch_add(1, Ordering::Relaxed);
        self.sum_ms.fetch_add((latency_ms * 1000.0) as u64, Ordering::Relaxed);
        for (i, threshold) in LATENCY_BUCKETS_MS.iter().enumerate() {
            if latency_ms <= *threshold {
                self.buckets[i].fetch_add(1, Ordering::Relaxed);
                break;
            }
        }
    }

    fn snapshot(&self) -> (u64, f64, Vec<u64>) {
        let count = self.count.load(Ordering::Relaxed);
        let sum_us = self.sum_ms.load(Ordering::Relaxed);
        let sum_ms = sum_us as f64 / 1000.0;
        let buckets: Vec<u64> = self.buckets.iter().map(|b| b.load(Ordering::Relaxed)).collect();
        (count, sum_ms, buckets)
    }
}

struct MetricsInner {
    requests_total: AtomicU64,
    errors_total: AtomicU64,
    bytes_in: AtomicU64,
    bytes_out: AtomicU64,
    active_connections: AtomicU64,
    status_2xx: AtomicU64,
    status_3xx: AtomicU64,
    status_4xx: AtomicU64,
    status_5xx: AtomicU64,
    latency: LatencyHist,
    extra_providers: Mutex<Vec<Box<dyn MetricProvider>>>,
}

impl Metrics {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(MetricsInner {
                requests_total: AtomicU64::new(0),
                errors_total: AtomicU64::new(0),
                bytes_in: AtomicU64::new(0),
                bytes_out: AtomicU64::new(0),
                active_connections: AtomicU64::new(0),
                status_2xx: AtomicU64::new(0),
                status_3xx: AtomicU64::new(0),
                status_4xx: AtomicU64::new(0),
                status_5xx: AtomicU64::new(0),
                latency: LatencyHist::new(),
                extra_providers: Mutex::new(Vec::new()),
            }),
        }
    }

    pub fn record_request(&self) {
        self.inner.requests_total.fetch_add(1, Ordering::Relaxed);
    }

    pub fn record_error(&self) {
        self.inner.errors_total.fetch_add(1, Ordering::Relaxed);
    }

    /// Record a response status code for status-breakdown tracking.
    pub fn record_status(&self, status: StatusCode) {
        let code: u16 = status.into();
        match code / 100 {
            2 => self.inner.status_2xx.fetch_add(1, Ordering::Relaxed),
            3 => self.inner.status_3xx.fetch_add(1, Ordering::Relaxed),
            4 => self.inner.status_4xx.fetch_add(1, Ordering::Relaxed),
            _ => self.inner.status_5xx.fetch_add(1, Ordering::Relaxed),
        };
    }

    /// Record request latency in milliseconds.
    pub fn record_latency(&self, ms: f64) {
        self.inner.latency.record(ms);
    }

    pub fn add_bytes_in(&self, n: u64) {
        self.inner.bytes_in.fetch_add(n, Ordering::Relaxed);
    }

    /// Register an extra Prometheus metric provider (e.g. scheduler stats).
    pub fn register_extra_provider(&self, provider: Box<dyn MetricProvider>) {
        self.inner.extra_providers.lock().unwrap().push(provider);
    }

    pub fn add_bytes_out(&self, n: u64) {
        self.inner.bytes_out.fetch_add(n, Ordering::Relaxed);
    }

    pub fn connection_opened(&self) {
        self.inner.active_connections.fetch_add(1, Ordering::Relaxed);
    }

    pub fn connection_closed(&self) {
        self.inner.active_connections.fetch_sub(1, Ordering::Relaxed);
    }

    /// Return an atomic snapshot of all metric values.
    pub fn snapshot(&self) -> MetricsSnapshot {
        let (latency_count, latency_sum_ms, latency_buckets) = self.inner.latency.snapshot();
        MetricsSnapshot {
            requests_total: self.inner.requests_total.load(Ordering::Relaxed),
            errors_total: self.inner.errors_total.load(Ordering::Relaxed),
            bytes_in: self.inner.bytes_in.load(Ordering::Relaxed),
            bytes_out: self.inner.bytes_out.load(Ordering::Relaxed),
            active_connections: self.inner.active_connections.load(Ordering::Relaxed),
            status_2xx: self.inner.status_2xx.load(Ordering::Relaxed),
            status_3xx: self.inner.status_3xx.load(Ordering::Relaxed),
            status_4xx: self.inner.status_4xx.load(Ordering::Relaxed),
            status_5xx: self.inner.status_5xx.load(Ordering::Relaxed),
            latency_count,
            latency_sum_ms,
            latency_buckets,
        }
    }

    /// Compute p50, p95, p99, p999 from the latency histogram snapshot.
    pub fn percentiles(&self) -> Option<Percentiles> {
        let (count, _sum, buckets) = self.inner.latency.snapshot();
        if count == 0 {
            return None;
        }
        let total = count as f64;
        let compute = |pct: f64| -> f64 {
            let target = total * pct;
            let mut cumulative = 0u64;
            for (i, &bucket_count) in buckets.iter().enumerate() {
                cumulative += bucket_count;
                if cumulative as f64 >= target {
                    return LATENCY_BUCKETS_MS[i.min(LATENCY_BUCKETS_MS.len() - 1)];
                }
            }
            *LATENCY_BUCKETS_MS.last().unwrap_or(&10000.0)
        };
        Some(Percentiles {
            p50: compute(0.50),
            p95: compute(0.95),
            p99: compute(0.99),
            p999: compute(0.999),
        })
    }

    /// Return Prometheus text format including histograms and status-code metrics.
    pub fn prometheus(&self) -> String {
        let s = self.snapshot();
        let mut out = String::new();

        out.push_str(
            "# HELP justapi_requests_total Total number of requests.\n\
             # TYPE justapi_requests_total counter\n",
        );
        out.push_str(&format!("justapi_requests_total {}\n", s.requests_total));

        out.push_str(
            "# HELP justapi_errors_total Total number of errors.\n\
             # TYPE justapi_errors_total counter\n",
        );
        out.push_str(&format!("justapi_errors_total {}\n", s.errors_total));

        out.push_str(
            "# HELP justapi_bytes_in_total Total bytes received.\n\
             # TYPE justapi_bytes_in_total counter\n",
        );
        out.push_str(&format!("justapi_bytes_in_total {}\n", s.bytes_in));

        out.push_str(
            "# HELP justapi_bytes_out_total Total bytes sent.\n\
             # TYPE justapi_bytes_out_total counter\n",
        );
        out.push_str(&format!("justapi_bytes_out_total {}\n", s.bytes_out));

        out.push_str(
            "# HELP justapi_active_connections Current active connections.\n\
             # TYPE justapi_active_connections gauge\n",
        );
        out.push_str(&format!("justapi_active_connections {}\n", s.active_connections));

        // Status code breakdown
        out.push_str(
            "# HELP justapi_requests_by_status Requests by HTTP status code class.\n\
             # TYPE justapi_requests_by_status counter\n",
        );
        out.push_str(&format!("justapi_requests_by_status{{code=\"2xx\"}} {}\n", s.status_2xx));
        out.push_str(&format!("justapi_requests_by_status{{code=\"3xx\"}} {}\n", s.status_3xx));
        out.push_str(&format!("justapi_requests_by_status{{code=\"4xx\"}} {}\n", s.status_4xx));
        out.push_str(&format!("justapi_requests_by_status{{code=\"5xx\"}} {}\n", s.status_5xx));

        // Latency histogram
        out.push_str(
            "# HELP justapi_request_duration_ms Request latency in milliseconds.\n\
             # TYPE justapi_request_duration_ms histogram\n",
        );
        out.push_str(&format!("justapi_request_duration_ms_count {}\n", s.latency_count));
        out.push_str(&format!("justapi_request_duration_ms_sum {:.3}\n", s.latency_sum_ms));
        for (i, bucket_count) in s.latency_buckets.iter().enumerate() {
            let le = LATENCY_BUCKETS_MS[i.min(LATENCY_BUCKETS_MS.len() - 1)];
            out.push_str(&format!(
                "justapi_request_duration_ms_bucket{{le=\"{}\"}} {}\n",
                le, bucket_count
            ));
        }
        out.push_str(&format!(
            "justapi_request_duration_ms_bucket{{le=\"+Inf\"}} {}\n",
            s.latency_count
        ));

        // Extra metric providers (e.g. scheduler prefix-cache stats).
        let providers = self.inner.extra_providers.lock().unwrap();
        for provider in providers.iter() {
            out.push_str(&provider.render());
        }

        out
    }
}

impl Default for Metrics {
    fn default() -> Self {
        Self::new()
    }
}

/// Latency percentiles computed from histogram data.
#[derive(Debug, Clone, Copy)]
pub struct Percentiles {
    pub p50: f64,
    pub p95: f64,
    pub p99: f64,
    pub p999: f64,
}

/// Health check endpoint response.
pub fn health_response() -> Response<ResponseBody> {
    let body = r#"{"status":"ok"}"#;
    let body_bytes = Full::new(Bytes::from(body))
        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(body_bytes))
        .unwrap()
}

/// Readiness check endpoint response.
pub fn ready_response() -> Response<ResponseBody> {
    let body = r#"{"ready":true}"#;
    let body_bytes = Full::new(Bytes::from(body))
        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(body_bytes))
        .unwrap()
}

/// Liveness check endpoint response.
pub fn live_response() -> Response<ResponseBody> {
    let body = r#"{"alive":true}"#;
    let body_bytes = Full::new(Bytes::from(body))
        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "application/json")
        .header("content-length", body.len().to_string())
        .body(UnsyncBoxBody::new(body_bytes))
        .unwrap()
}

/// Prometheus metrics endpoint response.
pub fn metrics_response(metrics: &Metrics) -> Response<ResponseBody> {
    let body = metrics.prometheus();
    let body_len = body.len().to_string();
    let body_bytes = Full::new(Bytes::from(body))
        .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} });
    Response::builder()
        .status(StatusCode::OK)
        .header("content-type", "text/plain; version=0.0.4")
        .header("content-length", body_len)
        .body(UnsyncBoxBody::new(body_bytes))
        .unwrap()
}

/// Timer guard that records elapsed time on drop.
pub struct RequestTimer {
    start: Instant,
}

impl RequestTimer {
    pub fn start() -> Self {
        Self { start: Instant::now() }
    }

    pub fn elapsed_ms(&self) -> f64 {
        self.start.elapsed().as_secs_f64() * 1000.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_prometheus() {
        let m = Metrics::new();
        m.record_request();
        m.record_request();
        m.record_error();
        m.record_status(StatusCode::OK);
        m.record_status(StatusCode::NOT_FOUND);
        m.record_latency(10.0);
        m.record_latency(20.0);
        let out = m.prometheus();
        assert!(out.contains("justapi_requests_total 2"));
        assert!(out.contains("justapi_errors_total 1"));
        assert!(out.contains("justapi_requests_by_status{code=\"2xx\"} 1"));
        assert!(out.contains("justapi_requests_by_status{code=\"4xx\"} 1"));
        assert!(out.contains("justapi_request_duration_ms_count 2"));
        assert!(out.contains("justapi_request_duration_ms_sum"));
        assert!(out.contains("justapi_request_duration_ms_bucket"));
    }

    #[test]
    fn test_metrics_snapshot() {
        let m = Metrics::new();
        m.record_request();
        m.record_status(StatusCode::OK);
        m.record_latency(5.0);
        let snap = m.snapshot();
        assert_eq!(snap.requests_total, 1);
        assert_eq!(snap.status_2xx, 1);
        assert_eq!(snap.latency_count, 1);
    }

    #[test]
    fn test_percentiles_empty() {
        let m = Metrics::new();
        assert!(m.percentiles().is_none());
    }

    #[test]
    fn test_percentiles() {
        let m = Metrics::new();
        for _ in 0..100 {
            m.record_latency(5.0);
        }
        let p = m.percentiles().unwrap();
        assert!(p.p50 <= 5.0);
        assert!(p.p95 <= 5.0);
    }

    #[test]
    fn test_health_response() {
        let resp = health_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_ready_response() {
        let resp = ready_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_live_response() {
        let resp = live_response();
        assert_eq!(resp.status(), StatusCode::OK);
    }

    #[test]
    fn test_request_timer() {
        let t = RequestTimer::start();
        std::thread::sleep(std::time::Duration::from_millis(10));
        assert!(t.elapsed_ms() >= 5.0);
    }

    #[test]
    fn test_status_code_breakdown() {
        let m = Metrics::new();
        m.record_status(StatusCode::OK);
        m.record_status(StatusCode::CREATED);
        m.record_status(StatusCode::FOUND);
        m.record_status(StatusCode::BAD_REQUEST);
        m.record_status(StatusCode::INTERNAL_SERVER_ERROR);
        let snap = m.snapshot();
        assert_eq!(snap.status_2xx, 2);
        assert_eq!(snap.status_3xx, 1);
        assert_eq!(snap.status_4xx, 1);
        assert_eq!(snap.status_5xx, 1);
    }
}
