use std::sync::Arc;

use futures::FutureExt;
use hyper::body::Incoming;
use hyper::{Request, StatusCode};

use crate::json_response;
use crate::middleware::HandlerFn;

/// Wraps a handler fn so that panics are caught, logged, and converted to 500 responses.
///
/// NOTE: We no longer manipulate the global panic hook per-request (that was a race
/// condition under concurrent load). The workspace `panic = "abort"` policy means
/// genuine panics terminate the process and the supervisor restarts it. This wrapper
/// uses `catch_unwind` to convert panics into 500 responses without touching global state.
pub fn with_panic_recovery(handler: HandlerFn) -> HandlerFn {
    Arc::new(move |req: Request<Incoming>| {
        let handler = handler.clone();
        Box::pin(async move {
            // SAFETY: The handler is wrapped in AssertUnwindSafe to catch panics.
            // The workspace `panic = "abort"` policy means genuine panics terminate
            // the process, so this is only needed for the catch_unwind path.
            // If the handler holds Rc/RefCell/&mut across the catch point, this
            // could lead to undefined states — but the handler is a Box<dyn Fn>
            // which is Send + Sync, so interior mutability is the only concern.
            let result = std::panic::AssertUnwindSafe(handler(req)).catch_unwind().await;

            match result {
                Ok(Ok(resp)) => Ok(resp),
                Ok(Err(e)) => {
                    tracing::error!(error = %e, "Handler returned error");
                    Ok(json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"detail":"Internal error"}"#,
                    ))
                }
                Err(panic) => {
                    let msg = if let Some(s) = panic.downcast_ref::<&str>() {
                        s.to_string()
                    } else if let Some(s) = panic.downcast_ref::<String>() {
                        s.clone()
                    } else {
                        "unknown panic".to_string()
                    };
                    tracing::error!(panic = true, message = %msg, "Handler panicked");
                    Ok(json_response(
                        StatusCode::INTERNAL_SERVER_ERROR,
                        r#"{"detail":"Internal server error"}"#,
                    ))
                }
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
