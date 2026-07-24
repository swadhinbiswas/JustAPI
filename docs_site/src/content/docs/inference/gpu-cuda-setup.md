---
title: GPU & CUDA Configuration
description: Configure GPU acceleration for LLM inference with CUDA on JustAPI — a high-performance FastAPI alternative built in Rust.
keywords: GPU acceleration, CUDA, LLM inference, FastAPI alternative, Rust web framework
---

> **Note:** GPU inference is feature-gated behind `cuda` and `real` features. Real GPU benchmarks are deferred to the 2.0.8 release (ADR-067). Structural CPU benchmarks pass.

## Building with CUDA Support

```bash
# Install CUDA toolkit (requires CUDA 12.x)
# Then build with CUDA features
cargo build --release --features "cuda,real"

# Or with the bench harness
cargo build --release -p justapi-bench --features cuda
```

## Running on GPU

```bash
justapi serve --model my-model.gguf --gpu
```

## GPU Memory Management

The inference engine uses PagedAttention for efficient GPU memory utilization:

```bash
justapi serve --model my-model.gguf --gpu \
  --pool-blocks 8192 \        # Total KV-cache blocks
  --max-seqs 64               # Maximum concurrent sequences
```

### Block Pool Configuration

| Flag | Default | Description |
|---|---|---|
| `--pool-blocks` | `4096` | Total blocks in the KV-cache pool |
| `--block-size` | `16` | Tokens per block |
| `--max-seqs` | `32` | Maximum concurrent sequences |

### Memory Calculation

Total KV-cache memory = `pool_blocks * block_size * num_layers * 2 * dtype_size`

For a 7B model with 32 layers, 16 block size, 4096 blocks:

```
4096 * 16 * 32 * 2 * 2 bytes (FP16) = 8 GB
```

## Multi-GPU

Multi-GPU support is in development. Currently, models are loaded on a single GPU.

## Quantization

JustAPI supports several quantization formats through GGUF:

| Format | Bits | Description |
|---|---|---|
| `FP16` | 16 | Full precision |
| `Q4_0` | 4 | 4-bit quants, no grouping |
| `Q4_K_M` | 4 | 4-bit with medium grouping |
| `Q5_K_M` | 5 | 5-bit with medium grouping |
| `Q8_0` | 8 | 8-bit quants |

```bash
justapi serve --model model-q4_k_m.gguf --gpu
```

## LoRA Adapters

```bash
justapi serve --model base-model.gguf --gpu \
  --lora adapter.safetensors \
  --lora-scale 0.8
```

Multiple LoRAs can be loaded and swapped at runtime via the control plane API.

## See Also

- [Inference Overview](/inference/overview/) — Architecture and components
- [LLM Serving API](/inference/llm-serving-api/) — API endpoints
- [Scheduling & Batching](/inference/scheduling-batching/) — Performance optimization
