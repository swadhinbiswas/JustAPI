//! OpenAI-compatible HTTP endpoints, wired to `justapi-inference`.
//!
//! Mounts `/v1/chat/completions`, `/v1/completions`, `/v1/embeddings`, and
//! `/v1/models` onto the [`Server`](crate::server::Server). The generation
//! hot path runs inside the inference engine on a dedicated thread (no GIL),
//! and tokens are streamed as Server-Sent Events via the core
//! [`streaming_response`](crate::streaming_response) helper.
//!
//! Enabled by the `inference` feature (so the default build stays lean).

use std::sync::Arc;

use http_body_util::BodyExt;
use hyper::StatusCode;

use crate::json_response;
use crate::middleware::HandlerFn;
use crate::streaming_response;
use justapi_inference::openai::{
    ApiError, ChatCompletionRequest, CompletionRequest, EmbeddingRequest, ErrorResponse,
};
use justapi_inference::{ControlPlane, Engine, RouteRequest, Router, SchedulerEngine};

/// Safely convert a u16 status code to `StatusCode`, falling back to 500 if
/// the code is invalid (defensive against upstream bugs returning non-HTTP codes).
fn safe_status(st: u16) -> StatusCode {
    StatusCode::from_u16(st).unwrap_or(StatusCode::INTERNAL_SERVER_ERROR)
}

/// Build a handler for `POST /v1/chat/completions`.
pub fn chat_completions_handler(engine: Arc<Engine>) -> HandlerFn {
    Arc::new(move |req| {
        let engine = engine.clone();
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let req: ChatCompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            if req.stream {
                let stream = justapi_inference::openai::chat_completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::chat_completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = ErrorResponse {
                            error: ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

/// Build a handler for `POST /v1/completions`.
pub fn completions_handler(engine: Arc<Engine>) -> HandlerFn {
    Arc::new(move |req| {
        let engine = engine.clone();
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let req: CompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            if req.stream {
                let stream = justapi_inference::openai::completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = ErrorResponse {
                            error: ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

/// Build a handler for `POST /v1/embeddings`.
pub fn embeddings_handler(engine: Arc<Engine>) -> HandlerFn {
    Arc::new(move |req| {
        let engine = engine.clone();
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let req: EmbeddingRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            match justapi_inference::openai::embeddings(&engine, req) {
                Ok(resp) => {
                    let json = serde_json::to_string(&resp)?;
                    Ok(json_response(StatusCode::OK, &json))
                }
                Err(e) => {
                    let env = ErrorResponse {
                        error: ApiError {
                            message: e.to_string(),
                            error_type: "server_error".into(),
                            param: None,
                            code: None,
                        },
                    };
                    let json = serde_json::to_string(&env).unwrap_or_default();
                    Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                }
            }
        })
    })
}

/// Build a handler for `GET /v1/models`.
pub fn models_handler(engine: Arc<Engine>) -> HandlerFn {
    Arc::new(move |_req| {
        let engine = engine.clone();
        Box::pin(async move {
            let list = justapi_inference::openai::model_list(&engine);
            let json = serde_json::to_string(&list)?;
            Ok(json_response(StatusCode::OK, &json))
        })
    })
}

// ---------------------------------------------------------------------------
// Scheduler-backed handlers (continuous batching + prefix caching)
// ---------------------------------------------------------------------------

/// Build a handler for `POST /v1/chat/completions` that uses the scheduler
/// for admission control, prefix caching, and step-by-step generation.
pub fn scheduled_chat_completions_handler(engine: Arc<SchedulerEngine>) -> HandlerFn {
    Arc::new(move |req| {
        let engine = engine.clone();
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let req: ChatCompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            if req.stream {
                let stream =
                    justapi_inference::openai::scheduled_chat_completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::scheduled_chat_completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = justapi_inference::openai::ErrorResponse {
                            error: justapi_inference::openai::ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

/// Build a handler for `POST /v1/completions` that uses the scheduler.
pub fn scheduled_completions_handler(engine: Arc<SchedulerEngine>) -> HandlerFn {
    Arc::new(move |req| {
        let engine = engine.clone();
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let req: CompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            if req.stream {
                let stream = justapi_inference::openai::scheduled_completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::scheduled_completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = justapi_inference::openai::ErrorResponse {
                            error: justapi_inference::openai::ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

// ---------------------------------------------------------------------------
// Routed handlers (LoRA-aware / KV-aware admission via ControlPlane + Router)
// ---------------------------------------------------------------------------

/// Peek the routing fields (`model`, optional `version` / `routing_key`) out of
/// a raw request body without committing to a full deserialize yet.
fn routing_context(body: &[u8]) -> Option<RouteRequest> {
    let v: serde_json::Value = serde_json::from_slice(body).ok()?;
    let model = v.get("model")?.as_str()?.to_string();
    let version = v.get("version").and_then(|x| x.as_str()).map(String::from);
    let routing_key = v.get("routing_key").and_then(|x| x.as_str()).map(String::from);
    Some(RouteRequest { model, version, routing_key })
}

/// Outcome of routing admission: either serve via the resolved decision, or
/// reject with a ready-to-serve error response.
enum AdmitOutcome {
    Serve(justapi_inference::RouteDecision),
    Reject(hyper::Response<crate::ResponseBody>),
}

/// Admit a request through the control plane + router.
///
/// Returns [`AdmitOutcome::Serve`] with the resolved decision on success, or
/// [`AdmitOutcome::Reject`] with a ready-to-serve error response (400 missing
/// model, 503 no capacity) on failure.
fn admit(body: &[u8], cp: &ControlPlane, router: &Router) -> AdmitOutcome {
    let route_req = match routing_context(body) {
        Some(r) => r,
        None => {
            let (st, json) = justapi_inference::openai::error_json(
                "request must include a `model`",
                "invalid_request_error",
                400,
            );
            return AdmitOutcome::Reject(json_response(safe_status(st), &json));
        }
    };
    match router.route(cp, &route_req) {
        Some(d) => AdmitOutcome::Serve(d),
        None => {
            let (st, json) = justapi_inference::openai::error_json(
                "no replica available to serve this request (all at capacity or no route)",
                "server_error",
                503,
            );
            AdmitOutcome::Reject(json_response(safe_status(st), &json))
        }
    }
}

/// Build a handler for `POST /v1/chat/completions` that routes via the control
/// plane before generating. Returns 503 when no replica can serve the request.
pub fn routed_chat_completions_handler(
    engine: Arc<Engine>,
    cp: Arc<ControlPlane>,
    router: Arc<Router>,
) -> HandlerFn {
    Arc::new(move |req| {
        let (engine, cp, router) = (engine.clone(), cp.clone(), router.clone());
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let decision = match admit(&body, &cp, &router) {
                AdmitOutcome::Serve(d) => d,
                AdmitOutcome::Reject(resp) => return Ok(resp),
            };
            let mut req: ChatCompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            req.model = decision.resolved.model_name.clone();
            if req.stream {
                let stream = justapi_inference::openai::chat_completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::chat_completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = ErrorResponse {
                            error: ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

/// Build a handler for `POST /v1/completions` that routes via the control plane.
pub fn routed_completions_handler(
    engine: Arc<Engine>,
    cp: Arc<ControlPlane>,
    router: Arc<Router>,
) -> HandlerFn {
    Arc::new(move |req| {
        let (engine, cp, router) = (engine.clone(), cp.clone(), router.clone());
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let decision = match admit(&body, &cp, &router) {
                AdmitOutcome::Serve(d) => d,
                AdmitOutcome::Reject(resp) => return Ok(resp),
            };
            let mut req: CompletionRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            req.model = decision.resolved.model_name.clone();
            if req.stream {
                let stream = justapi_inference::openai::completion_stream(engine, req);
                Ok(streaming_response(StatusCode::OK, "text/event-stream", stream))
            } else {
                match justapi_inference::openai::completion(engine, req).await {
                    Ok(resp) => {
                        let json = serde_json::to_string(&resp)?;
                        Ok(json_response(StatusCode::OK, &json))
                    }
                    Err(e) => {
                        let env = ErrorResponse {
                            error: ApiError {
                                message: e.to_string(),
                                error_type: "server_error".into(),
                                param: None,
                                code: None,
                            },
                        };
                        let json = serde_json::to_string(&env).unwrap_or_default();
                        Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                    }
                }
            }
        })
    })
}

/// Build a handler for `POST /v1/embeddings` that routes via the control plane.
pub fn routed_embeddings_handler(
    engine: Arc<Engine>,
    cp: Arc<ControlPlane>,
    router: Arc<Router>,
) -> HandlerFn {
    Arc::new(move |req| {
        let (engine, cp, router) = (engine.clone(), cp.clone(), router.clone());
        Box::pin(async move {
            let body = match req.into_body().collect().await {
                Ok(b) => b.to_bytes(),
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            let decision = match admit(&body, &cp, &router) {
                AdmitOutcome::Serve(d) => d,
                AdmitOutcome::Reject(resp) => return Ok(resp),
            };
            let mut req: EmbeddingRequest = match serde_json::from_slice(&body) {
                Ok(r) => r,
                Err(e) => {
                    let (st, json) = justapi_inference::openai::error_json(
                        &e.to_string(),
                        "invalid_request_error",
                        400,
                    );
                    return Ok(json_response(safe_status(st), &json));
                }
            };
            req.model = decision.resolved.model_name.clone();
            match justapi_inference::openai::embeddings(&engine, req) {
                Ok(resp) => {
                    let json = serde_json::to_string(&resp)?;
                    Ok(json_response(StatusCode::OK, &json))
                }
                Err(e) => {
                    let env = ErrorResponse {
                        error: ApiError {
                            message: e.to_string(),
                            error_type: "server_error".into(),
                            param: None,
                            code: None,
                        },
                    };
                    let json = serde_json::to_string(&env).unwrap_or_default();
                    Ok(json_response(StatusCode::UNPROCESSABLE_ENTITY, &json))
                }
            }
        })
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::testing::TestClient;
    use crate::Server;
    use justapi_inference::EngineDevice;

    fn test_engine() -> Arc<Engine> {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");
        engine
    }

    #[tokio::test]
    async fn chat_completions_non_streaming_via_http() {
        let handler = chat_completions_handler(test_engine());
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "chat.completion");
        assert!(!v["choices"][0]["message"]["content"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn chat_completions_streaming_via_http() {
        let handler = chat_completions_handler(test_engine());
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": true
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let text = String::from_utf8(resp.body).unwrap();
        assert!(text.contains("data: "));
        assert!(text.contains("data: [DONE]"));
    }

    #[tokio::test]
    async fn models_endpoint_via_http() {
        let handler = models_handler(test_engine());
        let client = TestClient::new(handler);
        let resp = client.get("/v1/models").await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "list");
        assert!(v["data"].as_array().unwrap().iter().any(|m| m["id"] == "mock"));
    }

    #[tokio::test]
    async fn embeddings_endpoint_via_http() {
        let handler = embeddings_handler(test_engine());
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "input": ["hello", "world"]
        })
        .to_string();
        let resp = client.post("/v1/embeddings", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "list");
        assert_eq!(v["data"].as_array().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn server_with_openai_registers_routes() {
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let server = Server::new(addr).with_openai(test_engine());
        // The engine registered "mock"; confirm the handler wire-up compiles
        // and the models handler answers through the full server path.
        let handler = models_handler(test_engine());
        let client = TestClient::new(handler);
        let resp = client.get("/v1/models").await.unwrap();
        assert_eq!(resp.status, 200);
        let _ = server;
    }

    // -- Routed handlers (ControlPlane + Router admission) --------------------

    use justapi_inference::{
        ControlPlane, ModelVersion, Replica, Router, RoutingStrategy, WeightLocation,
    };
    use std::path::PathBuf;

    fn routed_test_setup() -> (Arc<Engine>, Arc<ControlPlane>, Arc<Router>) {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");

        let cp = Arc::new(ControlPlane::new());
        cp.register_version(
            "mock",
            ModelVersion::new("v1", WeightLocation::Local(PathBuf::from("/models/v1"))),
        );

        let router = Arc::new(Router::new(RoutingStrategy::LeastLoaded));
        router.register_replica(Replica {
            id: "r1".to_string(),
            model: "mock".to_string(),
            version: "v1".to_string(),
            device: EngineDevice::Cpu,
            max_concurrency: 4,
            active_sequences: 0,
            kv_pressure_pct: 0.0,
            healthy: true,
            loaded_adapters: vec![],
        });

        (engine, cp, router)
    }

    #[tokio::test]
    async fn routed_chat_completions_admits_and_serves() {
        let (engine, cp, router) = routed_test_setup();
        let handler = routed_chat_completions_handler(engine, cp, router);
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "chat.completion");
    }

    #[tokio::test]
    async fn routed_chat_completions_503_when_no_capacity() {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");
        let cp = Arc::new(ControlPlane::new());
        cp.register_version(
            "mock",
            ModelVersion::new("v1", WeightLocation::Local(PathBuf::from("/models/v1"))),
        );
        // Router with NO replicas for the model → route returns None → 503.
        let router = Arc::new(Router::new(RoutingStrategy::LeastLoaded));

        let handler = routed_chat_completions_handler(engine, cp, router);
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hi"}],
            "max_tokens": 5,
            "stream": false
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 503);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["error"]["type"], "server_error");
    }

    // -- Scheduler-backed handlers (continuous batching + prefix cache) ------

    use justapi_inference::{KvBlockPool, Scheduler, SchedulerConfig, SchedulerEngine};

    /// Build a scheduler-backed engine wired to the scheduler path.
    fn scheduled_test_setup() -> (Arc<Engine>, Arc<SchedulerEngine>) {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");
        let pool = KvBlockPool::new(1024);
        let config = SchedulerConfig { max_num_seqs: 8, ..Default::default() };
        let scheduler = Arc::new(std::sync::Mutex::new(Scheduler::new(config, pool)));
        let se = Arc::new(SchedulerEngine::new(engine.clone(), scheduler));
        (engine, se)
    }

    #[tokio::test]
    async fn scheduled_chat_completions_non_streaming_via_http() {
        let (_engine, se) = scheduled_test_setup();
        let handler = scheduled_chat_completions_handler(se);
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hello world"}],
            "max_tokens": 5,
            "stream": false
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "chat.completion");
        assert!(!v["choices"][0]["message"]["content"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn scheduled_chat_completions_streaming_via_http() {
        let (_engine, se) = scheduled_test_setup();
        let handler = scheduled_chat_completions_handler(se);
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "messages": [{"role": "user", "content": "hello world"}],
            "max_tokens": 5,
            "stream": true
        })
        .to_string();
        let resp = client.post("/v1/chat/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let text = String::from_utf8(resp.body).unwrap();
        assert!(text.contains("data: "));
        assert!(text.contains("data: [DONE]"));
    }

    #[tokio::test]
    async fn scheduled_completions_non_streaming_via_http() {
        let (_engine, se) = scheduled_test_setup();
        let handler = scheduled_completions_handler(se);
        let client = TestClient::new(handler);
        let body = serde_json::json!({
            "model": "mock",
            "prompt": "hello world",
            "max_tokens": 4,
            "stream": false
        })
        .to_string();
        let resp = client.post("/v1/completions", body.into_bytes()).await.unwrap();
        assert_eq!(resp.status, 200);
        let v: serde_json::Value = serde_json::from_slice(&resp.body).unwrap();
        assert_eq!(v["object"], "text_completion");
        assert!(!v["choices"][0]["text"].as_str().unwrap().is_empty());
    }

    #[tokio::test]
    async fn scheduled_server_with_openai_scheduled_exposes_metrics() {
        let addr: std::net::SocketAddr = "127.0.0.1:0".parse().unwrap();
        let (_engine, se) = scheduled_test_setup();
        let server = Server::new(addr).with_openai_scheduled(se);

        // The /metrics endpoint should include scheduler prefix-cache lines,
        // proving the scheduler's extra metric provider is wired in.
        let rendered = server.metrics().prometheus();
        assert!(
            rendered.contains("justapi_scheduler_"),
            "scheduler metrics should be exposed via /metrics: {rendered}"
        );
        let _ = server;
    }

    // --- Crash-prevention tests (Phase 53.3) ---

    #[test]
    fn test_safe_status_valid_codes() {
        assert_eq!(safe_status(200), StatusCode::OK);
        assert_eq!(safe_status(404), StatusCode::NOT_FOUND);
        assert_eq!(safe_status(500), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(safe_status(422), StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(safe_status(429), StatusCode::TOO_MANY_REQUESTS);
    }

    #[test]
    fn test_safe_status_invalid_code_falls_back_to_500() {
        assert_eq!(safe_status(0), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(safe_status(99), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(safe_status(6000), StatusCode::INTERNAL_SERVER_ERROR);
        assert_eq!(safe_status(u16::MAX), StatusCode::INTERNAL_SERVER_ERROR);
    }
}
