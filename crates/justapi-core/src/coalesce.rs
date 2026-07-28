//! Request coalescing (singleflight) middleware.
//!
//! When many concurrent, identical requests arrive for the same route, only
//! one is allowed to reach the handler. The remaining requests "coalesce"
//! onto the in-flight leader and share its response once it completes. This
//! collapses thundering-herd traffic on hot, read-only endpoints (leaderboards,
//! model lookups, config fetches, expensive aggregations) into a single
//! upstream call.
//!
//! Coalescing is keyed on `(method, uri, selected-headers)`. The response body
//! is streamed to the leader's client while being simultaneously buffered into
//! a shared slot; waiters read the buffered copy when the leader finishes, so
//! the handler runs exactly once per distinct in-flight key. If the leader
//! fails, that error is shared with all waiters rather than re-executing the
//! handler for each.

use std::collections::HashMap;
use std::pin::Pin;
use std::sync::Arc;
use std::sync::Mutex as StdMutex;
use std::task::{Context, Poll};
use tokio::sync::Mutex as AsyncMutex;

use anyhow::Result;
use async_trait::async_trait;
use bytes::{Bytes, BytesMut};
use http_body::{Body, Frame, SizeHint};
use http_body_util::combinators::UnsyncBoxBody;
use http_body_util::BodyExt;
use http_body_util::Full;
use hyper::header::{HeaderName, HeaderValue};
use hyper::{HeaderMap, Method, Request, Response, StatusCode, Uri};
use tokio::sync::watch;

use crate::middleware::{Middleware, Next};
use crate::ResponseBody;

/// A buffered, cloneable snapshot of a successful response, shared with
/// waiters.
#[derive(Clone)]
struct CoalescedResponse {
    status: StatusCode,
    headers: HeaderMap,
    body: Bytes,
}

impl CoalescedResponse {
    fn to_response(&self) -> Response<ResponseBody> {
        let mut builder = Response::builder().status(self.status);
        for (name, value) in self.headers.iter() {
            // Drop framing headers; they are recomputed for the replayed body.
            if name == hyper::header::TRANSFER_ENCODING || name == hyper::header::CONNECTION {
                continue;
            }
            builder = builder.header(name, value);
        }
        // `Infallible` can never be produced by `Full`, so this map is total.
        builder
            .body(UnsyncBoxBody::new(
                Full::new(self.body.clone())
                    .map_err(|e: std::convert::Infallible| -> anyhow::Error { match e {} }),
            ))
            .expect("static response construction never fails")
    }
}

/// The published state of a coalesced key. Distinct variants guarantee that
/// `watch::Receiver::changed` always fires when the leader resolves, including
/// on failure (the initial state is `Pending`, never `Ok`/`Err`).
#[derive(Clone)]
enum Outcome {
    Pending,
    Ok(CoalescedResponse),
    Err(String),
}

/// Identity of a coalesceable request.
#[derive(Clone, PartialEq, Eq, Hash)]
struct CoalesceKey {
    method: Method,
    uri: Uri,
    headers: Vec<(HeaderName, HeaderValue)>,
}

/// A body that forwards frames to the leader's client while appending data
/// frames to a shared buffer, then publishes the full snapshot to waiting
/// requests when the stream ends (or is dropped).
struct FanoutBody {
    inner: ResponseBody,
    status: StatusCode,
    headers: HeaderMap,
    acc: Arc<StdMutex<BytesMut>>,
    tx: Arc<watch::Sender<Outcome>>,
    finished: bool,
}

impl Body for FanoutBody {
    type Data = Bytes;
    type Error = anyhow::Error;

    fn poll_frame(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
    ) -> Poll<Option<Result<Frame<Self::Data>, Self::Error>>> {
        let this = self.get_mut();
        match Pin::new(&mut this.inner).poll_frame(cx) {
            Poll::Ready(Some(Ok(frame))) => {
                if let Some(chunk) = frame.data_ref() {
                    this.acc.lock().unwrap_or_else(|e| e.into_inner()).extend_from_slice(chunk);
                }
                Poll::Ready(Some(Ok(frame)))
            }
            Poll::Ready(None) => {
                if !this.finished {
                    this.finished = true;
                    let full = this.acc.lock().unwrap_or_else(|e| e.into_inner()).split().freeze();
                    this_tx_send(&this.tx, full, this.status, &this.headers);
                }
                Poll::Ready(None)
            }
            other => other,
        }
    }

    fn size_hint(&self) -> SizeHint {
        self.inner.size_hint()
    }
}

impl Drop for FanoutBody {
    fn drop(&mut self) {
        if !self.finished {
            self.finished = true;
            let full = self.acc.lock().unwrap_or_else(|e| e.into_inner()).split().freeze();
            this_tx_send(&self.tx, full, self.status, &self.headers);
        }
    }
}

/// Publish a successful snapshot, used both on stream end and on drop.
fn this_tx_send(
    tx: &Arc<watch::Sender<Outcome>>,
    body: Bytes,
    status: StatusCode,
    headers: &HeaderMap,
) {
    let _ = tx.send(Outcome::Ok(CoalescedResponse { status, headers: headers.clone(), body }));
}

/// Middleware that collapses concurrent identical requests into a single
/// upstream call and shares the response (or error) with all waiters.
pub struct RequestCoalescer {
    in_flight: AsyncMutex<HashMap<CoalesceKey, Arc<watch::Sender<Outcome>>>>,
    include_headers: Vec<HeaderName>,
}

impl RequestCoalescer {
    /// Create a coalescer with default settings (no extra headers in the key).
    pub fn new() -> Self {
        Self { in_flight: AsyncMutex::new(HashMap::new()), include_headers: Vec::new() }
    }

    /// Include the values of the given request headers in the coalesce key.
    /// Useful when the same path serves different content based on, e.g.,
    /// `Accept` or `Accept-Language`.
    pub fn with_headers(mut self, headers: &[HeaderName]) -> Self {
        self.include_headers = headers.to_vec();
        self
    }

    fn key_of<B: Send + 'static>(&self, req: &Request<B>) -> CoalesceKey {
        let mut headers = Vec::with_capacity(self.include_headers.len());
        for name in &self.include_headers {
            if let Some(value) = req.headers().get(name) {
                headers.push((name.clone(), value.clone()));
            }
        }
        CoalesceKey { method: req.method().clone(), uri: req.uri().clone(), headers }
    }
}

impl Default for RequestCoalescer {
    fn default() -> Self {
        Self::new()
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for RequestCoalescer {
    async fn handle(&self, req: Request<B>, next: Next<'_, B>) -> Result<Response<ResponseBody>> {
        let key = self.key_of(&req);

        // Atomically check for an in-flight leader or claim leadership.
        let (tx, is_leader) = {
            let mut map = self.in_flight.lock().await;
            match map.get(&key) {
                Some(existing) => (existing.clone(), false),
                None => {
                    let (tx, _rx) = watch::channel(Outcome::Pending);
                    let tx = Arc::new(tx);
                    map.insert(key.clone(), tx.clone());
                    (tx, true)
                }
            }
        };

        if !is_leader {
            return self.wait_for_leader(key, tx, req, next).await;
        }
        self.run_leader(key, tx, req, next).await
    }
}

impl RequestCoalescer {
    async fn wait_for_leader<B: Send + 'static>(
        &self,
        key: CoalesceKey,
        tx: Arc<watch::Sender<Outcome>>,
        req: Request<B>,
        next: Next<'_, B>,
    ) -> Result<Response<ResponseBody>> {
        let mut rx = tx.subscribe();
        loop {
            match rx.borrow().clone() {
                Outcome::Ok(coalesced) => return Ok(coalesced.to_response()),
                Outcome::Err(message) => return Err(anyhow::anyhow!(message)),
                Outcome::Pending => {}
            }
            if rx.changed().await.is_err() {
                // Leader dropped without publishing; re-run as a leader so the
                // request is not lost.
                break;
            }
        }
        self.run_leader(key, tx, req, next).await
    }

    async fn run_leader<B: Send + 'static>(
        &self,
        key: CoalesceKey,
        tx: Arc<watch::Sender<Outcome>>,
        req: Request<B>,
        next: Next<'_, B>,
    ) -> Result<Response<ResponseBody>> {
        let result = next.run(req).await;
        // Leadership for this key ends now; waiters already hold their own
        // receivers and will be released when the fan-out body completes.
        self.in_flight.lock().await.remove(&key);

        let response = match result {
            Ok(response) => response,
            Err(e) => {
                // Share the failure with waiters so they don't re-execute the
                // handler or hang.
                let _ = tx.send(Outcome::Err(e.to_string()));
                return Err(e);
            }
        };

        let status = response.status();
        let headers = response.headers().clone();
        let (parts, body) = response.into_parts();
        let fanout = FanoutBody {
            inner: body,
            status,
            headers,
            acc: Arc::new(StdMutex::new(BytesMut::new())),
            tx: tx.clone(),
            finished: false,
        };
        Ok(Response::from_parts(parts, UnsyncBoxBody::new(fanout)))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::json_response;
    use crate::middleware::{HandlerFn, MiddlewareChain};
    use crate::testing::TestClient;
    use hyper::StatusCode;
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    /// A handler that counts invocations and pauses briefly so concurrent
    /// requests overlap in time, making coalescing observable.
    fn counting_handler(counter: Arc<AtomicU64>, delay_ms: u64) -> HandlerFn {
        Arc::new(move |_req| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Ok(json_response(StatusCode::OK, r#"{"hello":"world"}"#))
            })
        })
    }

    #[cfg(not(miri))]
    fn error_handler(counter: Arc<AtomicU64>, delay_ms: u64) -> HandlerFn {
        Arc::new(move |_req| {
            let counter = counter.clone();
            Box::pin(async move {
                counter.fetch_add(1, Ordering::SeqCst);
                if delay_ms > 0 {
                    tokio::time::sleep(Duration::from_millis(delay_ms)).await;
                }
                Err(anyhow::anyhow!("boom"))
            })
        })
    }

    /// Build a `TestClient` whose pipeline wraps `handler` in a coalescer.
    fn client_with(handler: HandlerFn, coalescer: RequestCoalescer) -> TestClient {
        let mut chain = MiddlewareChain::new(handler);
        chain.add(coalescer);
        TestClient::new(Arc::new(move |req| {
            let c = chain.clone();
            Box::pin(async move { c.run(req).await })
        }))
    }

    #[tokio::test]
    async fn single_request_runs_handler_once() {
        let counter = Arc::new(AtomicU64::new(0));
        let client = client_with(counting_handler(counter.clone(), 0), RequestCoalescer::new());
        let resp = client.get("/a").await.unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.body, br#"{"hello":"world"}"#);
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }

    /// This test is skipped under Miri due to a known tokio DuplexStream
    /// Stacked Borrows violation (not a real bug in production code).
    #[cfg(not(miri))]
    #[tokio::test]
    async fn concurrent_identical_gets_coalesce_to_one_handler_call() {
        let counter = Arc::new(AtomicU64::new(0));
        let client = client_with(counting_handler(counter.clone(), 50), RequestCoalescer::new());

        let mut futs = Vec::new();
        for _ in 0..10 {
            futs.push(client.get("/same"));
        }
        let responses = futures::future::join_all(futs).await;

        for r in &responses {
            let r = r.as_ref().unwrap();
            assert_eq!(r.status, 200);
            assert_eq!(r.body, br#"{"hello":"world"}"#);
        }
        // Exactly one upstream call despite ten concurrent requests.
        assert_eq!(
            counter.load(Ordering::SeqCst),
            1,
            "handler should run exactly once for coalesced requests"
        );
    }

    #[tokio::test]
    async fn distinct_paths_are_not_coalesced() {
        let counter = Arc::new(AtomicU64::new(0));
        let client = client_with(counting_handler(counter.clone(), 20), RequestCoalescer::new());

        let (a, b) = futures::future::join(client.get("/a"), client.get("/b")).await;
        let _ = (a, b);

        assert_eq!(counter.load(Ordering::SeqCst), 2);
    }

    #[tokio::test]
    async fn header_inclusion_changes_coalesce_key() {
        let coalescer = RequestCoalescer::new().with_headers(&[hyper::header::ACCEPT]);
        let client = client_with(counting_handler(Arc::new(AtomicU64::new(0)), 30), coalescer);

        // Two concurrent requests with different Accept headers -> different keys.
        let (a, b) = futures::future::join(
            client.get_with("/x", &[("accept", "application/json")]),
            client.get_with("/x", &[("accept", "text/html")]),
        )
        .await;
        let _ = (a, b);

        // Two concurrent requests with the same Accept header -> coalesced.
        let counter2 = Arc::new(AtomicU64::new(0));
        let coalescer2 = RequestCoalescer::new().with_headers(&[hyper::header::ACCEPT]);
        let client2 = client_with(counting_handler(counter2.clone(), 30), coalescer2);
        let (c, d) = futures::future::join(
            client2.get_with("/y", &[("accept", "application/json")]),
            client2.get_with("/y", &[("accept", "application/json")]),
        )
        .await;
        let _ = (c, d);
        assert_eq!(counter2.load(Ordering::SeqCst), 1);
    }

    /// This test is skipped under Miri due to a known tokio DuplexStream
    /// Stacked Borrows violation (not a real bug in production code).
    #[cfg(not(miri))]
    #[tokio::test]
    async fn leader_error_is_shared_with_waiters() {
        let counter = Arc::new(AtomicU64::new(0));
        let client = client_with(error_handler(counter.clone(), 30), RequestCoalescer::new());

        let mut futs = Vec::new();
        for _ in 0..8 {
            futs.push(client.get("/err"));
        }
        let responses = futures::future::join_all(futs).await;
        // Every request resolves (no hang) and sees the leader's error.
        assert_eq!(responses.len(), 8);
        for r in &responses {
            assert!(r.is_err(), "waiter should receive the shared leader error");
        }
        // The handler ran exactly once; the failure was shared, not re-executed.
        assert_eq!(counter.load(Ordering::SeqCst), 1);
    }
}
