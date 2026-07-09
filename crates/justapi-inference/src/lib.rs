//! # justapi-inference
//!
//! Native Rust inference engine for JustAPI — the "own the GPU path" layer that
//! makes JustAPI a better model-serving framework than FastAPI (which delegates
//! all GPU work to `torch` under the GIL).
//!
//! **Phase 41 (foundation):** crate + Candle integration, device management,
//! a `Model` trait, a streaming `Engine` with an in-memory model registry, and a
//! CPU-runnable `MockModel` so the whole pipeline is exercised without weights.
//!
//! Real weight loading + the KV-cache-backed forward pass land in Phase 42
//! (behind the `real` feature, gated on having model weights + a GPU toolkit).
//!
//! Design notes:
//! - Generation runs on a dedicated OS thread (Candle compute is synchronous);
//!   tokens are streamed back over a tokio `mpsc` channel. The hot path is
//!   GIL-free Rust — exactly the property FastAPI cannot offer.
//! - The `Model` trait is the extension point: `MockModel` for tests/demos,
//!   `LlamaModel` (Phase 42) for real inference.

mod autoscaler;
mod control_plane;
mod engine;
mod gateway;
mod kv_cache;
mod model;
pub mod openai;
pub mod pd;
pub mod radix_cache;
pub mod real;
mod router;
mod scheduler;
mod scheduler_engine;
mod spec_decode;
mod spec_decode_tree;
mod supervisor;

pub use autoscaler::{Autoscaler, AutoscalerConfig, LlmMetrics, ScaleDecision};
pub use control_plane::{
    ControlPlane, ModelRecord, ModelVersion, ResolvedModel, RuntimeProfile, WeightLocation,
};
pub use engine::{Engine, EngineDevice, ModelRegistry, ModelSource};
pub use gateway::{GatewayConfig, GatewayDecision, InferenceGateway};
pub use kv_cache::{
    BlockId, KvBlockPool, PoolStats, PrefixCache, PrefixCacheStats, Sequence, BLOCK_SIZE,
    MAX_BLOCKS,
};
pub use model::{FinishReason, GeneratedToken, MockModel, Model, ModelError, SamplingParams};
pub use openai::{
    ApiError, ChatCompletionChunk, ChatCompletionRequest, ChatCompletionResponse, ChatMessage,
    ChatMessageOut, CompletionChoice, CompletionRequest, CompletionResponse, Delta, EmbeddingInput,
    EmbeddingObject, EmbeddingRequest, EmbeddingResponse, ErrorResponse, ModelList, ModelObject,
    ModelPermission, Usage,
};
pub use pd::{PdScheduler, PdStats};
pub use radix_cache::{RadixPrefixCache, RadixPrefixCacheStats};
pub use real::lora::{LoraAdapter, LoraConfig, LoraRegistry};
pub use real::quant::gguf::GgufType as RawGgufType;
pub use real::quant::{LayerQuantConfig, QuantConfig, QuantMethod};
pub use router::{Replica, RouteDecision, RouteRequest, Router, RoutingStrategy};
pub use scheduler::TransferableSequence;
pub use scheduler::{
    NewRequest, PrefillStep, Schedule, Scheduler, SchedulerConfig, SchedulerStats,
};
pub use scheduler_engine::SchedulerEngine;
pub use spec_decode::{speculative_generate, AcceptanceStats, SpeculativeConfig, SpeculativeModel};
pub use spec_decode_tree::{
    build_draft_tree, speculative_generate_tree, top_k_tokens, verify_tree, DraftTree, TreeNode,
    TreeSpeculativeModel,
};
pub use supervisor::{LiveReplica, Supervisor, SupervisorAction, SupervisorConfig};

/// Default vocabulary size used by [`MockModel`] when none is supplied.
pub const DEFAULT_MOCK_VOCAB: usize = 32_000;
