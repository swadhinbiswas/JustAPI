# Serve an LLM over the OpenAI API

Run a real GGUF model through JustAPI's OpenAI-compatible endpoints — the
whole serving stack (HTTP, SSE streaming, scheduling) is Rust; the model runs
on Candle.

> Status: verified on CPU (TinyLlama-1.1B Q4_K_M, 2026-10-10). GPU (`cuda`
> feature) is untested on the dev box; no GPU performance claims.

## 1. Build

```bash
cargo build --release -p justapi-cli --features inference,real
```

(`real` pulls candle-transformers + tokenizers; skip it to serve a
weight-free MockModel for endpoint testing.)

## 2. Get a model

Any **llama-architecture** GGUF works (tokenizer is read from GGUF metadata;
`tokenizer.json` is not required). Examples:

- TinyLlama-1.1B-Chat Q4_K_M (~670 MB)
- Llama-3.2-1B-Instruct Q4_K_M (~800 MB)

Put the `.gguf` file in a directory:

```bash
mkdir -p ~/models/tinyllama
cp tinyllama-1.1b-chat-v1.0.Q4_K_M.gguf ~/models/tinyllama/
```

## 3. Serve

```bash
./target/release/justapi-cli serve \
  --addr 0.0.0.0:8080 \
  --model tinyllama \
  --model-path ~/models/tinyllama \
  --gpu cpu
```

The log line `Serving REAL model via OpenAI-compatible endpoints` confirms
real weights were loaded. `--workers N` also works (each worker loads its own
copy — memory is per worker).

## 4. Call it

```bash
# Model list
curl -s http://127.0.0.1:8080/v1/models | jq

# Chat completion (non-streaming)
curl -s http://127.0.0.1:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"tinyllama","messages":[{"role":"user","content":"The capital of France is"}],"max_tokens":16,"temperature":0}' | jq -r '.choices[0].message.content'

# Streaming (SSE)
curl -N http://127.0.0.1:8080/v1/chat/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"tinyllama","messages":[{"role":"user","content":"The capital of France is"}],"max_tokens":10,"temperature":0,"stream":true}'

# Text completion
curl -s http://127.0.0.1:8080/v1/completions \
  -H 'Content-Type: application/json' \
  -d '{"model":"tinyllama","prompt":"Once upon a time","max_tokens":16,"temperature":0}' | jq -r '.choices[0].text'
```

Any OpenAI SDK works — point `base_url` at `http://127.0.0.1:8080/v1`.

## Notes & current limits

- **Prompt formatting:** chat templates are not applied yet; `chat/completions`
  flattens messages (`role: content`) like the base-completion format.
  Instruct-model quality is best with `/v1/completions` or after a future
  chat-template pass.
- **Stop tokens:** EOS from GGUF metadata is not wired yet — generation runs
  to `max_tokens`.
- **Throughput:** single-stream CPU decode (~17–25 tok/s for 1.1B Q4 on a
  20-thread i5-13600K). Continuous batching (`--scheduled`) is wired but not
  yet measured with real weights.
- **Speculative decoding** (`--gamma/--branch`) is MockModel-only for now;
  it is ignored (with a warning) when serving real weights.
