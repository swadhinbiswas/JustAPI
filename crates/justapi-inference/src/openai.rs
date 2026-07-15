//! OpenAI-compatible request/response protocol for JustAPI inference.
//!
//! Provides the data types and streaming/non-streaming drivers that make a
//! [`Engine`](crate::engine::Engine) speak the OpenAI Chat Completions,
//! Completions, Embeddings, and Models APIs. The hot path (token generation)
//! runs inside the engine on a dedicated OS thread; this module only formats
//! the I/O — it never touches the GIL.
//!
//! The HTTP-wiring (mounting these as routes) lives in `justapi-core`'s
//! `openai` module behind the `inference` feature, so this crate stays
//! dependency-light and testable on its own.

use std::sync::Arc;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::engine::Engine;
use crate::model::{FinishReason, MockModel, ModelError, SamplingParams};
use crate::scheduler_engine::SchedulerEngine;

// ---------------------------------------------------------------------------
// Request types
// ---------------------------------------------------------------------------

/// A single chat message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatMessage {
    pub role: String,
    pub content: String,
    #[serde(default)]
    pub name: Option<String>,
}

fn default_max_tokens() -> usize {
    256
}

/// `POST /v1/chat/completions` request.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ChatCompletionRequest {
    /// Model registry name (the name the model was registered under).
    pub model: String,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    /// Newer OpenAI clients send this instead of `max_tokens`.
    #[serde(default)]
    pub max_completion_tokens: Option<usize>,
    #[serde(default)]
    pub top_p: Option<f32>,
    /// Stream tokens as Server-Sent Events.
    #[serde(default)]
    pub stream: bool,
    /// Stop strings (ignored by the mock model; honored by real models).
    #[serde(default)]
    pub stop: Option<Vec<String>>,
    #[serde(default)]
    pub user: Option<String>,
}

impl ChatCompletionRequest {
    /// Resolve the effective max-token count across both fields.
    pub fn effective_max_tokens(&self) -> usize {
        self.max_completion_tokens.unwrap_or(self.max_tokens)
    }
}

/// `POST /v1/completions` request.
#[derive(Debug, Clone, Deserialize)]
pub struct CompletionRequest {
    pub model: String,
    pub prompt: String,
    #[serde(default)]
    pub temperature: Option<f32>,
    #[serde(default = "default_max_tokens")]
    pub max_tokens: usize,
    #[serde(default)]
    pub top_p: Option<f32>,
    #[serde(default)]
    pub stream: bool,
    #[serde(default)]
    pub stop: Option<Vec<String>>,
}

/// `POST /v1/embeddings` request.
#[derive(Debug, Clone, Deserialize)]
pub struct EmbeddingRequest {
    pub model: String,
    #[serde(default)]
    pub input: EmbeddingInput,
    #[serde(default)]
    pub encoding_format: Option<String>,
}

/// Embedding input: a single string or a list of strings.
#[derive(Debug, Clone, Deserialize)]
#[serde(untagged)]
pub enum EmbeddingInput {
    Single(String),
    Many(Vec<String>),
}

impl Default for EmbeddingInput {
    fn default() -> Self {
        EmbeddingInput::Single(String::new())
    }
}

impl EmbeddingInput {
    fn into_texts(self) -> Vec<String> {
        match self {
            EmbeddingInput::Single(s) => vec![s],
            EmbeddingInput::Many(v) => v,
        }
    }
}

// ---------------------------------------------------------------------------
// Response types
// ---------------------------------------------------------------------------

/// An OpenAI `model` object.
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModelObject {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub owned_by: String,
    #[serde(default)]
    pub permission: Vec<ModelPermission>,
    #[serde(default)]
    pub root: Option<String>,
    #[serde(default)]
    pub parent: Option<String>,
}

/// OpenAI model permission entry (placeholder).
#[derive(Debug, Clone, Serialize, Default)]
pub struct ModelPermission {
    #[serde(default)]
    pub id: String,
    #[serde(default)]
    pub object: &'static str,
    #[serde(default)]
    pub created: u64,
    #[serde(default)]
    pub allow_create_engine: bool,
    #[serde(default)]
    pub allow_sampling: bool,
    #[serde(default)]
    pub allow_logprobs: bool,
    #[serde(default)]
    pub allow_search_indices: bool,
    #[serde(default)]
    pub allow_view: bool,
    #[serde(default)]
    pub organization: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub is_blocking: bool,
}

/// `GET /v1/models` response.
#[derive(Debug, Clone, Serialize)]
pub struct ModelList {
    pub object: &'static str,
    pub data: Vec<ModelObject>,
}

/// Token usage accounting.
#[derive(Debug, Clone, Copy, Serialize)]
pub struct Usage {
    pub prompt_tokens: usize,
    pub completion_tokens: usize,
    pub total_tokens: usize,
}

/// `POST /v1/chat/completions` (non-streaming) response.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChatCompletionChoice>,
    pub usage: Usage,
}

/// A single chat completion choice.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionChoice {
    pub index: usize,
    pub message: ChatMessageOut,
    pub finish_reason: String,
}

/// An assistant message in a completion response.
#[derive(Debug, Clone, Serialize)]
pub struct ChatMessageOut {
    pub role: String,
    pub content: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// A streaming chat-completion chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChatCompletionChunk {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<ChunkChoice>,
}

/// A single choice delta within a streaming chunk.
#[derive(Debug, Clone, Serialize)]
pub struct ChunkChoice {
    pub index: usize,
    pub delta: Delta,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub finish_reason: Option<String>,
}

/// The incremental content of a streaming chunk.
#[derive(Debug, Clone, Serialize, Default)]
pub struct Delta {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub content: Option<String>,
}

/// `POST /v1/completions` (non-streaming) response.
#[derive(Debug, Clone, Serialize)]
pub struct CompletionResponse {
    pub id: String,
    pub object: &'static str,
    pub created: u64,
    pub model: String,
    pub choices: Vec<CompletionChoice>,
    pub usage: Usage,
}

/// A single text-completion choice.
#[derive(Debug, Clone, Serialize)]
pub struct CompletionChoice {
    pub text: String,
    pub index: usize,
    pub finish_reason: String,
}

/// A single embedding vector entry.
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingObject {
    pub object: &'static str,
    pub index: usize,
    pub embedding: Vec<f32>,
}

/// `POST /v1/embeddings` response.
#[derive(Debug, Clone, Serialize)]
pub struct EmbeddingResponse {
    pub object: &'static str,
    pub data: Vec<EmbeddingObject>,
    pub model: String,
    pub usage: Usage,
}

/// OpenAI-style error envelope.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorResponse {
    pub error: ApiError,
}

/// OpenAI-style error body.
#[derive(Debug, Clone, Serialize)]
pub struct ApiError {
    pub message: String,
    #[serde(rename = "type")]
    pub error_type: String,
    #[serde(default)]
    pub param: Option<String>,
    #[serde(default)]
    pub code: Option<String>,
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

const SSE_DONE: &str = "data: [DONE]\n\n";

/// Map a finish reason to its OpenAI string.
pub fn finish_reason_str(r: FinishReason) -> &'static str {
    match r {
        FinishReason::Length => "length",
        FinishReason::Stop => "stop",
        FinishReason::Cancelled => "stop",
    }
}

/// Flatten chat messages into a single prompt string.
pub fn prompt_from_messages(messages: &[ChatMessage]) -> String {
    let mut s = String::new();
    for m in messages {
        s.push_str(&m.role);
        s.push_str(": ");
        s.push_str(&m.content);
        s.push('\n');
    }
    s
}

fn now_secs() -> u64 {
    SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_secs()).unwrap_or(0)
}

fn short_id() -> String {
    let nanos = SystemTime::now().duration_since(UNIX_EPOCH).map(|d| d.as_nanos()).unwrap_or(0);
    format!("{:x}", nanos % 0xfffffffffffffff)
}

/// Format a chunk as an SSE `data:` event.
pub fn sse_event(chunk: &ChatCompletionChunk) -> String {
    let json = serde_json::to_string(chunk).expect("chunk serializes");
    format!("data: {json}\n\n")
}

/// Build an error SSE event from an API error.
pub fn sse_error(err: &ApiError) -> String {
    let env = ErrorResponse { error: err.clone() };
    let json = serde_json::to_string(&env).expect("error serializes");
    format!("data: {json}\n\n")
}

/// Build a JSON error response string.
pub fn error_json(message: &str, error_type: &str, status: u16) -> (u16, String) {
    let env = ErrorResponse {
        error: ApiError {
            message: message.to_string(),
            error_type: error_type.to_string(),
            param: None,
            code: None,
        },
    };
    (status, serde_json::to_string(&env).unwrap_or_default())
}

// ---------------------------------------------------------------------------
// Drivers
// ---------------------------------------------------------------------------

/// Stream chat-completion tokens as OpenAI SSE events.
///
/// The returned stream is `'static` (it owns an `Arc<Engine>`), so it can be
/// handed straight to a streaming HTTP response body.
pub fn chat_completion_stream(
    engine: Arc<Engine>,
    req: ChatCompletionRequest,
) -> impl futures::Stream<Item = Result<bytes::Bytes, anyhow::Error>> + 'static {
    let model = req.model.clone();
    let prompt = prompt_from_messages(&req.messages);
    let tokens = MockModel::tokenize(&prompt);
    let params = SamplingParams {
        max_tokens: req.effective_max_tokens(),
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };
    let id = format!("chatcmpl-{}", short_id());
    let created = now_secs();

    async_stream::stream! {
        // First event carries the assistant role.
        let role_chunk = ChatCompletionChunk {
            id: id.clone(),
            object: "chat.completion.chunk",
            created,
            model: model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: Some("assistant".into()), content: None },
                finish_reason: None,
            }],
        };
        yield Ok(bytes::Bytes::from(sse_event(&role_chunk)));

        let mut rx = match engine.generate(&model, &tokens, params) {
            Ok(rx) => rx,
            Err(e) => {
                let err = ApiError {
                    message: e.to_string(),
                    error_type: "server_error".into(),
                    param: None,
                    code: None,
                };
                yield Ok(bytes::Bytes::from(sse_error(&err)));
                yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
                return;
            }
        };

        let mut finished = false;
        while let Some(tok) = rx.recv().await {
            let finish_reason = tok.finish_reason.map(finish_reason_str).map(String::from);
            finished = finish_reason.is_some();
            let chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: Some(tok.text) },
                    finish_reason,
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&chunk)));
        }

        // Final event signals completion. If a stop token already ended the
        // stream, no extra finish chunk is needed.
        if !finished {
            let done_chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: None },
                    finish_reason: Some("length".into()),
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&done_chunk)));
        }
        yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
    }
}

/// Produce a full (non-streaming) chat completion.
pub async fn chat_completion(
    engine: Arc<Engine>,
    req: ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ModelError> {
    let model = req.model.clone();
    let prompt = prompt_from_messages(&req.messages);
    let tokens = MockModel::tokenize(&prompt);
    let prompt_tokens = tokens.len();
    let params = SamplingParams {
        max_tokens: req.effective_max_tokens(),
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };

    let mut rx = engine.generate(&model, &tokens, params)?;
    let mut content = String::new();
    let mut finish = "length";
    let mut completion_tokens = 0usize;
    while let Some(tok) = rx.recv().await {
        content.push_str(&tok.text);
        completion_tokens += 1;
        if let Some(r) = tok.finish_reason {
            finish = finish_reason_str(r);
        }
    }

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", short_id()),
        object: "chat.completion",
        created: now_secs(),
        model,
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: ChatMessageOut { role: "assistant".into(), content, finish_reason: None },
            finish_reason: finish.into(),
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    })
}

/// Stream text-completion tokens as OpenAI SSE events.
pub fn completion_stream(
    engine: Arc<Engine>,
    req: CompletionRequest,
) -> impl futures::Stream<Item = Result<bytes::Bytes, anyhow::Error>> + 'static {
    let model = req.model.clone();
    let tokens = MockModel::tokenize(&req.prompt);
    let params = SamplingParams {
        max_tokens: req.max_tokens,
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };
    let id = format!("cmpl-{}", short_id());
    let created = now_secs();

    async_stream::stream! {
        let mut rx = match engine.generate(&model, &tokens, params) {
            Ok(rx) => rx,
            Err(e) => {
                let err = ApiError {
                    message: e.to_string(),
                    error_type: "server_error".into(),
                    param: None,
                    code: None,
                };
                yield Ok(bytes::Bytes::from(sse_error(&err)));
                yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
                return;
            }
        };
        let mut finished = false;
        while let Some(tok) = rx.recv().await {
            let finish_reason = tok.finish_reason.map(finish_reason_str).map(String::from);
            finished = finish_reason.is_some();
            let chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: Some(tok.text) },
                    finish_reason,
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&chunk)));
        }
        if !finished {
            let done_chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: None },
                    finish_reason: Some("length".into()),
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&done_chunk)));
        }
        yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
    }
}

/// Produce a full (non-streaming) text completion.
pub async fn completion(
    engine: Arc<Engine>,
    req: CompletionRequest,
) -> Result<CompletionResponse, ModelError> {
    let model = req.model.clone();
    let tokens = MockModel::tokenize(&req.prompt);
    let prompt_tokens = tokens.len();
    let params = SamplingParams {
        max_tokens: req.max_tokens,
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };

    let mut rx = engine.generate(&model, &tokens, params)?;
    let mut text = String::new();
    let mut finish = "length";
    let mut completion_tokens = 0usize;
    while let Some(tok) = rx.recv().await {
        text.push_str(&tok.text);
        completion_tokens += 1;
        if let Some(r) = tok.finish_reason {
            finish = finish_reason_str(r);
        }
    }

    Ok(CompletionResponse {
        id: format!("cmpl-{}", short_id()),
        object: "text_completion",
        created: now_secs(),
        model,
        choices: vec![CompletionChoice { text, index: 0, finish_reason: finish.into() }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    })
}

/// Produce embeddings for the request inputs (mock: deterministic from tokens).
pub fn embeddings(engine: &Engine, req: EmbeddingRequest) -> Result<EmbeddingResponse, ModelError> {
    let _ = engine; // embeddings are model-independent in the mock path
    let texts = req.input.into_texts();
    let mut total_tokens = 0usize;
    let data = texts
        .into_iter()
        .enumerate()
        .map(|(i, text)| {
            let tokens = MockModel::tokenize(&text);
            total_tokens += tokens.len();
            // Deterministic pseudo-embedding: normalize token ids.
            let embedding: Vec<f32> = tokens.iter().map(|&t| (t as f32) / 1000.0 - 0.5).collect();
            EmbeddingObject { object: "embedding", index: i, embedding }
        })
        .collect();

    Ok(EmbeddingResponse {
        object: "list",
        data,
        model: req.model,
        usage: Usage { prompt_tokens: total_tokens, completion_tokens: 0, total_tokens },
    })
}

// ---------------------------------------------------------------------------
// Scheduler-backed drivers
// ---------------------------------------------------------------------------

/// Produce a full (non-streaming) chat completion via the scheduler.
pub async fn scheduled_chat_completion(
    engine: Arc<SchedulerEngine>,
    req: ChatCompletionRequest,
) -> Result<ChatCompletionResponse, ModelError> {
    let model = req.model.clone();
    let prompt = prompt_from_messages(&req.messages);
    let tokens = MockModel::tokenize(&prompt);
    let prompt_tokens = tokens.len();
    let params = SamplingParams {
        max_tokens: req.effective_max_tokens(),
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };

    let mut rx = engine.generate(&model, &tokens, params)?;
    let mut content = String::new();
    let mut finish = "length";
    let mut completion_tokens = 0usize;
    while let Some(tok) = rx.recv().await {
        content.push_str(&tok.text);
        completion_tokens += 1;
        if let Some(r) = tok.finish_reason {
            finish = finish_reason_str(r);
        }
    }

    Ok(ChatCompletionResponse {
        id: format!("chatcmpl-{}", short_id()),
        object: "chat.completion",
        created: now_secs(),
        model,
        choices: vec![ChatCompletionChoice {
            index: 0,
            message: ChatMessageOut { role: "assistant".into(), content, finish_reason: None },
            finish_reason: finish.into(),
        }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    })
}

/// Stream chat-completion tokens as OpenAI SSE events via the scheduler.
pub fn scheduled_chat_completion_stream(
    engine: Arc<SchedulerEngine>,
    req: ChatCompletionRequest,
) -> impl futures::Stream<Item = Result<bytes::Bytes, anyhow::Error>> + 'static {
    let model = req.model.clone();
    let prompt = prompt_from_messages(&req.messages);
    let tokens = MockModel::tokenize(&prompt);
    let params = SamplingParams {
        max_tokens: req.effective_max_tokens(),
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };
    let id = format!("chatcmpl-{}", short_id());
    let created = now_secs();

    async_stream::stream! {
        let role_chunk = ChatCompletionChunk {
            id: id.clone(),
            object: "chat.completion.chunk",
            created,
            model: model.clone(),
            choices: vec![ChunkChoice {
                index: 0,
                delta: Delta { role: Some("assistant".into()), content: None },
                finish_reason: None,
            }],
        };
        yield Ok(bytes::Bytes::from(sse_event(&role_chunk)));

        let mut rx = match engine.generate(&model, &tokens, params) {
            Ok(rx) => rx,
            Err(e) => {
                let err = ApiError {
                    message: e.to_string(),
                    error_type: "server_error".into(),
                    param: None,
                    code: None,
                };
                yield Ok(bytes::Bytes::from(sse_error(&err)));
                yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
                return;
            }
        };

        let mut finished = false;
        while let Some(tok) = rx.recv().await {
            let finish_reason = tok.finish_reason.map(finish_reason_str).map(String::from);
            if finish_reason.is_some() {
                finished = true;
            }
            let chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: Some(tok.text) },
                    finish_reason,
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&chunk)));
        }

        if !finished {
            let done_chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: None },
                    finish_reason: Some("length".into()),
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&done_chunk)));
        }
        yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
    }
}

/// Produce a full (non-streaming) text completion via the scheduler.
pub async fn scheduled_completion(
    engine: Arc<SchedulerEngine>,
    req: CompletionRequest,
) -> Result<CompletionResponse, ModelError> {
    let model = req.model.clone();
    let tokens = MockModel::tokenize(&req.prompt);
    let prompt_tokens = tokens.len();
    let params = SamplingParams {
        max_tokens: req.max_tokens,
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };

    let mut rx = engine.generate(&model, &tokens, params)?;
    let mut text = String::new();
    let mut finish = "length";
    let mut completion_tokens = 0usize;
    while let Some(tok) = rx.recv().await {
        text.push_str(&tok.text);
        completion_tokens += 1;
        if let Some(r) = tok.finish_reason {
            finish = finish_reason_str(r);
        }
    }

    Ok(CompletionResponse {
        id: format!("cmpl-{}", short_id()),
        object: "text_completion",
        created: now_secs(),
        model,
        choices: vec![CompletionChoice { text, index: 0, finish_reason: finish.into() }],
        usage: Usage {
            prompt_tokens,
            completion_tokens,
            total_tokens: prompt_tokens + completion_tokens,
        },
    })
}

/// Stream text-completion tokens as OpenAI SSE events via the scheduler.
pub fn scheduled_completion_stream(
    engine: Arc<SchedulerEngine>,
    req: CompletionRequest,
) -> impl futures::Stream<Item = Result<bytes::Bytes, anyhow::Error>> + 'static {
    let model = req.model.clone();
    let tokens = MockModel::tokenize(&req.prompt);
    let params = SamplingParams {
        max_tokens: req.max_tokens,
        temperature: req.temperature.unwrap_or(1.0),
        top_p: req.top_p.unwrap_or(1.0),
        ..Default::default()
    };
    let id = format!("cmpl-{}", short_id());
    let created = now_secs();

    async_stream::stream! {
        let mut rx = match engine.generate(&model, &tokens, params) {
            Ok(rx) => rx,
            Err(e) => {
                let err = ApiError {
                    message: e.to_string(),
                    error_type: "server_error".into(),
                    param: None,
                    code: None,
                };
                yield Ok(bytes::Bytes::from(sse_error(&err)));
                yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
                return;
            }
        };
        let mut finished = false;
        while let Some(tok) = rx.recv().await {
            let finish_reason = tok.finish_reason.map(finish_reason_str).map(String::from);
            if finish_reason.is_some() {
                finished = true;
            }
            let chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: Some(tok.text) },
                    finish_reason,
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&chunk)));
        }
        if !finished {
            let done_chunk = ChatCompletionChunk {
                id: id.clone(),
                object: "chat.completion.chunk",
                created,
                model: model.clone(),
                choices: vec![ChunkChoice {
                    index: 0,
                    delta: Delta { role: None, content: None },
                    finish_reason: Some("length".into()),
                }],
            };
            yield Ok(bytes::Bytes::from(sse_event(&done_chunk)));
        }
        yield Ok(bytes::Bytes::from_static(SSE_DONE.as_bytes()));
    }
}

/// List models registered in the engine.
pub fn model_list(engine: &Engine) -> ModelList {
    let data = engine
        .list_models()
        .into_iter()
        .map(|id| ModelObject {
            id,
            object: "model",
            created: now_secs(),
            owned_by: "justapi".into(),
            permission: vec![ModelPermission::default()],
            root: None,
            parent: None,
        })
        .collect();
    ModelList { object: "list", data }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::EngineDevice;
    use crate::model::FinishReason;
    use futures::StreamExt;

    fn test_engine() -> Arc<Engine> {
        let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
        engine.register_mock("mock");
        engine
    }

    fn chat_req(stream: bool) -> ChatCompletionRequest {
        ChatCompletionRequest {
            model: "mock".into(),
            messages: vec![ChatMessage {
                role: "user".into(),
                content: "hello world".into(),
                name: None,
            }],
            temperature: Some(0.0),
            max_tokens: 10,
            max_completion_tokens: None,
            top_p: None,
            stream,
            stop: None,
            user: None,
        }
    }

    #[test]
    fn protocol_types_roundtrip() {
        let req = chat_req(false);
        let json = serde_json::to_string(&req).unwrap();
        let back: ChatCompletionRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(back.model, "mock");
        assert_eq!(back.messages.len(), 1);
    }

    #[tokio::test]
    async fn chat_completion_non_streaming() {
        let engine = test_engine();
        let resp = chat_completion(engine, chat_req(false)).await.unwrap();
        assert_eq!(resp.object, "chat.completion");
        assert_eq!(resp.choices.len(), 1);
        assert_eq!(resp.choices[0].message.role, "assistant");
        assert!(!resp.choices[0].message.content.is_empty());
        assert_eq!(resp.choices[0].finish_reason, "length");
        assert!(resp.usage.total_tokens > 0);
    }

    #[tokio::test]
    async fn chat_completion_streaming_format() {
        let engine = test_engine();
        let mut stream = Box::pin(chat_completion_stream(engine, chat_req(true)));

        // First event: role.
        let first = stream.next().await.unwrap().unwrap();
        let first = String::from_utf8(first.to_vec()).unwrap();
        assert!(first.starts_with("data: "));
        assert!(first.contains("\"role\":\"assistant\""));

        // Subsequent events: content deltas, then finish, then [DONE].
        let mut saw_content = false;
        let mut saw_done = false;
        while let Some(item) = stream.next().await {
            let s = String::from_utf8(item.unwrap().to_vec()).unwrap();
            if s.contains("\"content\"") {
                saw_content = true;
            }
            if s == SSE_DONE {
                saw_done = true;
            }
        }
        assert!(saw_content);
        assert!(saw_done);
    }

    #[tokio::test]
    async fn text_completion_works() {
        let engine = test_engine();
        let req = CompletionRequest {
            model: "mock".into(),
            prompt: "hi".into(),
            temperature: Some(0.0),
            max_tokens: 8,
            top_p: None,
            stream: false,
            stop: None,
        };
        let resp = completion(engine, req).await.unwrap();
        assert_eq!(resp.object, "text_completion");
        assert!(!resp.choices[0].text.is_empty());
    }

    #[test]
    fn embeddings_work() {
        let engine = test_engine();
        let req = EmbeddingRequest {
            model: "mock".into(),
            input: EmbeddingInput::Many(vec!["a".into(), "bb".into()]),
            encoding_format: None,
        };
        let resp = embeddings(&engine, req).unwrap();
        assert_eq!(resp.data.len(), 2);
        assert_eq!(resp.data[0].embedding.len(), 1);
        assert_eq!(resp.data[1].embedding.len(), 2);
    }

    #[test]
    fn model_list_reflects_registry() {
        let engine = test_engine();
        let list = model_list(&engine);
        assert_eq!(list.object, "list");
        assert!(list.data.iter().any(|m| m.id == "mock"));
    }

    #[test]
    fn finish_reason_mapping() {
        assert_eq!(finish_reason_str(FinishReason::Stop), "stop");
        assert_eq!(finish_reason_str(FinishReason::Length), "length");
        assert_eq!(finish_reason_str(FinishReason::Cancelled), "stop");
    }

    #[test]
    fn prompt_from_messages_flattened() {
        let prompt = prompt_from_messages(&[
            ChatMessage { role: "system".into(), content: "be nice".into(), name: None },
            ChatMessage { role: "user".into(), content: "hi".into(), name: None },
        ]);
        assert!(prompt.contains("system: be nice"));
        assert!(prompt.contains("user: hi"));
    }
}
