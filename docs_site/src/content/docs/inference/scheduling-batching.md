---
title: Scheduling & Batching
description: Continuous batching and scheduling strategies for LLM inference.
---

JustAPI's inference scheduler implements continuous batching, prefix caching, and disaggregated prefill/decode for maximum LLM throughput.

## Continuous Batching

Unlike static batching (wait for all sequences to finish), continuous batching interleaves prefill and decode steps:

```
Time →  |--- Batch 1 (prefill) ---|---- Batch 1 (decode) ----|
         |--- Batch 2 (prefill) ---|---- Batch 2 (decode) ----|
                                    |--- Batch 3 (prefill) ---|
```

This eliminates the "wait for straggler" problem and maximizes GPU utilization.

## Scheduler Configuration

```bash
justapi serve --model my-model.gguf \
  --max-seqs 64 \             # Max concurrent sequences
  --pool-blocks 8192 \        # KV-cache pool size
  --scheduled                 # Use continuous batching scheduler
```

### Scheduler Parameters

| Parameter | Default | Description |
|---|---|---|
| `max_seqs` | `32` | Maximum concurrent sequences |
| `max_prefill` | `4` | Maximum concurrent prefills |
| `max_decode` | `32` | Maximum concurrent decodes |
| `chunk_size` | `512` | Prefill chunk size (tokens) |

## Prefill/Decode Disaggregation

The disaggregated scheduler (`PdScheduler`) separates prefill and decode onto dedicated resource pools:

- **Prefill pool:** Handles prompt processing (compute-bound)
- **Decode pool:** Handles token generation (memory-bound)

```
┌──────────────┐    ┌──────────────┐
│ Prefill Pool │    │ Decode Pool  │
│ (GPU batch)  │───▶│ (GPU batch)  │
└──────────────┘    └──────────────┘
```

This prevents decode starvation during heavy prefill loads.

## Prefix Caching (RadixAttention)

The radix prefix cache (`RadixPrefixCache`) reuses KV-cache prefixes across requests:

```
Request 1: "What is the capital of France?"
Request 2: "What is the capital of Germany?"
            └─────────────────┘
             Shared prefix: "What is the capital of"
```

The shared prefix is computed once and reused, saving prefill computation.

### Cache Statistics

The `/metrics` endpoint exposes prefix cache statistics:

| Metric | Description |
|---|---|
| `prefix_cache_hits` | Number of prefix cache hits |
| `prefix_cache_misses` | Number of prefix cache misses |
| `prefix_cache_hit_ratio` | Hit ratio (hits / total) |
| `prefix_cache_blocks` | Currently cached blocks |

## Speculative Decoding

### Single-Token Speculation

A smaller draft model proposes tokens, and the target model verifies them in parallel:

```
Draft:        [a] [b] [c] [d]
Target:       ✓   ✓   ✗   —
Accepted:     [a] [b] [c] → then resample
```

### Tree-Based Speculation (Medusa/EAGLE)

The tree-based decoder (`spec_decode_tree`) constructs a draft tree and verifies multiple branches simultaneously:

```bash
justapi serve --model my-model.gguf \
  --gamma 4 \                 # Speculation window size
  --branch 2                  # Tree branching factor
```

## CLI Configuration for Inference

```bash
justapi serve --model llama-7b.gguf \
  --gpu \
  --scheduled \
  --max-seqs 64 \
  --pool-blocks 4096 \
  --gamma 4 \
  --branch 2
```

## See Also

- [Inference Overview](/inference/overview/) — Architecture and components
- [LLM Serving API](/inference/llm-serving-api/) — API endpoints
- [GPU & CUDA Setup](/inference/gpu-cuda-setup/) — Hardware configuration
