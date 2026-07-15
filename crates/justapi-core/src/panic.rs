use std::sync::Arc;

use futures::FutureExt;
use hyper::body::Incoming;
use hyper::{Request, StatusCode};

use crate::json_response;
use crate::middleware::HandlerFn;

/// Wraps a handler fn so that panics are caught, logged, and converted to 500 responses.
pub fn with_panic_recovery(handler: HandlerFn) -> HandlerFn {
    Arc::new(move |req: Request<Incoming>| {
        let handler = handler.clone();
        Box::pin(async move {
            let prev = std::panic::take_hook();
            std::panic::set_hook(Box::new(|info| {
                let msg = info
                    .payload()
                    .downcast_ref::<&str>()
                    .map(|s| s.to_string())
                    .or_else(|| info.payload().downcast_ref::<String>().cloned())
                    .unwrap_or_else(|| "unknown panic".to_string());
                let location = info.location().map(|l| l.to_string()).unwrap_or_default();
                tracing::error!(
                    panic = true,
                    message = %msg,
                    location = %location,
                    "Handler panicked"
                );
            }));

            let result = std::panic::AssertUnwindSafe(handler(req)).catch_unwind().await;

            std::panic::set_hook(prev);

            match result {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "Handler returned error");
                    Ok(json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"error":"Internal error"}"#,
                    ))
                }
                Err(_panic) => Ok(json_response(
                    StatusCode::INTERNAL_SERVER_ERROR,
                    r#"{"error":"Internal server error"}"#,
                )),
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_with_panic_recovery_returns_handler_fn() {
        let handler: HandlerFn = Arc::new(|_req: Request<Incoming>| {
            Box::pin(async { Ok(json_response(StatusCode::OK, "ok")) })
        });
        let wrapped = with_panic_recovery(handler);
        // Smoke test: the wrapper is itself a HandlerFn with the same signature.
        let _wrapped_ref: &HandlerFn = &wrapped;
    }
}
