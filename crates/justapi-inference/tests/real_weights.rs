//! Real-weights smoke test — first step of the Phase-52 un-freeze.
//!
//! Requires `--features real` and `JUSTAPI_REAL_MODEL_DIR` pointing at a
//! directory containing a llama-architecture **GGUF** file (the tokenizer is
//! extracted from GGUF metadata). Ignored by default because weights are not
//! in CI; run:
//!
//! ```bash
//! JUSTAPI_REAL_MODEL_DIR=/path/to/model \
//!   cargo test -p justapi-inference --features real --test real_weights \
//!     -- --ignored --nocapture
//! ```
#![cfg(feature = "real")]

use std::path::PathBuf;
use std::time::Instant;

use justapi_inference::{Engine, EngineDevice, SamplingParams};

#[test]
#[ignore = "requires JUSTAPI_REAL_MODEL_DIR with a GGUF model"]
fn real_weights_generate_smoke() {
    let dir = std::env::var("JUSTAPI_REAL_MODEL_DIR")
        .expect("set JUSTAPI_REAL_MODEL_DIR to a directory containing a .gguf model");
    let engine = Engine::new(EngineDevice::Cpu).expect("engine");

    let t0 = Instant::now();
    engine.load("tiny", &PathBuf::from(&dir)).expect("load real model");
    println!("[smoke] load: {:.2}s", t0.elapsed().as_secs_f64());

    let prompt = "The capital of France is";
    let ids = engine.encode("tiny", prompt).expect("encode");
    println!("[smoke] prompt tokens: {} ({:?})", ids.len(), ids);

    let params = SamplingParams { max_tokens: 32, temperature: 0.0, ..Default::default() };
    let t1 = Instant::now();
    let mut rx = engine.generate_text("tiny", prompt, params).expect("generate");

    let mut n = 0usize;
    let mut text = String::new();
    let mut ttft = None;
    while let Some(tok) = rx.blocking_recv() {
        if ttft.is_none() {
            ttft = Some(t1.elapsed().as_secs_f64());
        }
        text.push_str(&tok.text);
        n += 1;
        if tok.finish_reason.is_some() {
            break;
        }
    }
    let dt = t1.elapsed().as_secs_f64();
    println!(
        "[smoke] generated {n} tokens in {dt:.2}s -> {:.1} tok/s (ttft {:.2}s)",
        n as f64 / dt,
        ttft.unwrap_or(0.0)
    );
    println!("[smoke] text: {text:?}");

    assert!(n > 0, "no tokens generated");
    let roundtrip = engine.decode("tiny", &ids).expect("decode");
    println!("[smoke] decode roundtrip: {roundtrip:?}");
    assert_eq!(roundtrip, prompt, "tokenizer roundtrip mismatch");
}
