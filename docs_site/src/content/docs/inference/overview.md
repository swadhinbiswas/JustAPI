---
title: Inference Engine Overview
description: JustAPI's native Rust inference engine for LLM serving — a high-performance FastAPI alternative built in Rust.
keywords: inference engine, LLM serving, FastAPI alternative, Rust web framework, machine learning
---

JustAPI includes a Rust-native inference engine (`justapi-inference`) built on the [Candle](https://github.com/huggingface/candle) ML framework. It provides efficient LLM serving with continuous batching, PagedAttention KV-cache, prefix caching, and speculative decoding.

> **Status:** The inference engine is feature-complete but **frozen** pending the 2.0.8 release (ADR-067). GPU benchmarks with real weights are deferred. CPU-based structural benchmarks pass.

## Architecture

```
┌───────────────────────────────────────────────┐
│              Control Plane                     │
│  Model Registry | Versioning | Autoscaler     │
└──────────────────┬────────────────────────────┘
                   │
┌──────────────────▼────────────────────────────┐
│              Router/Gateway                    │
│  Route by model | KV-pressure | TTFT          │
└──────────────────┬────────────────────────────┘
                   │
┌──────────────────▼────────────────────────────┐
│           Scheduler Engine                     │
│  Continuous Batching | Prefill/Decode         │
│  Prefix Cache | Back-pressure                │
└──────────────────┬────────────────────────────┘
                   │
┌──────────────────▼────────────────────────────┐
│         KV Cache Manager                      │
│  PagedAttention | Block Pool | Eviction      │
└──────────────────┬────────────────────────────┘
                   │
┌──────────────────▼────────────────────────────┐
│         Model Engine                          │
│  Candle (CPU/CUDA) | Quantization | LoRA     │
└───────────────────────────────────────────────┘
```

## Key Components

### Engine
The core model execution layer. Supports CPU and CUDA backends via Candle. Models are loaded from HuggingFace format or GGUF quantized files.

### Scheduler
Continuous batching scheduler that interleaves prefill and decode steps for maximum throughput. Integrates with the prefix cache for KV-cache reuse.

### KV Cache (PagedAttention)
Page-based KV-cache allocation that eliminates fragmentation. Uses a block pool allocator with clock eviction for efficient memory management.

### Prefix Cache (RadixAttention)
Radix-tree based prefix cache that reuses KV-cache prefixes across requests, significantly reducing prefill time for common prompt prefixes.

### Speculative Decoding
Draft-target verification for 2-3x higher acceptance rates. Supports both single-token and tree-based speculation (Medusa/EAGLE style).

### Control Plane
Model registry, versioning, autoscaler, and multi-replica supervisor for production LLM deployments.

## Crate Structure

The inference engine lives in `crates/justapi-inference/`:

| Module | Description |
|---|---|
| `engine` | Model loading and inference execution |
| `scheduler` | Continuous batching scheduler |
| `scheduler_engine` | Integration layer between Engine and Scheduler |
| `kv_cache` | PagedAttention KV-cache manager |
| `radix_cache` | Radix-tree prefix cache |
| `pd` | Disaggregated prefill/decode scheduler |
| `spec_decode` | Single-token speculative decoding |
| `spec_decode_tree` | Tree-based speculative decoding (Medusa/EAGLE) |
| `control_plane` | Model registry, versioning, autoscaler |
| `gateway` | Inference gateway for routing |
| `router` | Model-aware request router |
| `supervisor` | Multi-replica model supervisor |
| `openai` | OpenAI-compatible API types |
| `real` | Real-weight model loading (quant, LoRA) |

## OpenAI-Compatible API

When the inference feature is enabled, JustAPI serves an OpenAI-compatible API:

| Endpoint | Description |
|---|---|
| `POST /v1/chat/completions` | Chat completions with streaming |
| `POST /v1/completions` | Text completions |
| `POST /v1/embeddings` | Embeddings |
| `GET /v1/models` | List available models |

## Status

The inference engine is fully implemented and structurally benchmarked. Real GPU validation with actual model weights is deferred to the 2.0.8 release cycle.

## See Also

- [LLM Serving API](/reference/inference/llm-serving-api/) — OpenAI-compatible endpoints
- [GPU & CUDA Setup](/reference/inference/gpu-cuda-setup/) — Hardware configuration
- [Scheduling & Batching](/reference/inference/scheduling-batching/) — Performance optimization
