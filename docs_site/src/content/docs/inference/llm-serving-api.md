---
title: LLM Serving API
description: OpenAI-compatible API endpoints for LLM inference with JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: LLM serving API, OpenAI compatible, FastAPI alternative, Rust web framework, inference API
---

When the `inference` feature is enabled, JustAPI exposes an OpenAI-compatible API for LLM serving.

## Enabling Inference

```bash
# Build with inference support
cargo build --release --features inference

# Run with a model
justapi serve --model my-model.gguf
```

## Chat Completions

```http
POST /v1/chat/completions
Content-Type: application/json

{
  "model": "my-model",
  "messages": [
    {"role": "system", "content": "You are a helpful assistant."},
    {"role": "user", "content": "What is Rust?"}
  ],
  "temperature": 0.7,
  "max_tokens": 256,
  "stream": false
}
```

### Response

```json
{
  "id": "chatcmpl-123",
  "object": "chat.completion",
  "created": 1721827200,
  "model": "my-model",
  "choices": [
    {
      "index": 0,
      "message": {
        "role": "assistant",
        "content": "Rust is a systems programming language..."
      },
      "finish_reason": "stop"
    }
  ],
  "usage": {
    "prompt_tokens": 24,
    "completion_tokens": 58,
    "total_tokens": 82
  }
}
```

### Streaming

```http
POST /v1/chat/completions
Content-Type: application/json

{
  "model": "my-model",
  "messages": [{"role": "user", "content": "Count to 5."}],
  "stream": true
}
```

Streaming response (SSE):

```
data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"role":"assistant"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"1"},"finish_reason":null}]}

data: {"id":"chatcmpl-123","object":"chat.completion.chunk","choices":[{"index":0,"delta":{"content":"2"},"finish_reason":null}]}

data: [DONE]
```

## Completions

```http
POST /v1/completions
Content-Type: application/json

{
  "model": "my-model",
  "prompt": "Once upon a time,",
  "max_tokens": 100,
  "temperature": 0.8
}
```

## Embeddings

```http
POST /v1/embeddings
Content-Type: application/json

{
  "model": "my-model",
  "input": "The quick brown fox jumps over the lazy dog"
}
```

## Model Management

```http
GET /v1/models
```

```json
{
  "object": "list",
  "data": [
    {
      "id": "my-model",
      "object": "model",
      "created": 1721827200,
      "owned_by": "justapi"
    }
  ]
}
```

## Sampling Parameters

| Parameter | Type | Default | Description |
|---|---|---|---|
| `temperature` | float | `1.0` | Sampling temperature |
| `top_p` | float | `1.0` | Nucleus sampling threshold |
| `top_k` | int | `40` | Top-k sampling |
| `max_tokens` | int | `256` | Maximum tokens to generate |
| `stop` | list[str] | `[]` | Stop sequences |
| `presence_penalty` | float | `0.0` | Penalize new tokens |
| `frequency_penalty` | float | `0.0` | Penalize frequent tokens |

## See Also

- [Inference Overview](/inference/overview/) — Architecture and components
- [GPU & CUDA Setup](/inference/gpu-cuda-setup/) — Hardware configuration
- [Scheduling & Batching](/inference/scheduling-batching/) — Performance optimization
