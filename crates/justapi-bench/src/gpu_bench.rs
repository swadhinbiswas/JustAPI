//! GPU wall-clock benchmark: naive `Engine::generate` vs scheduler-backed
//! `SchedulerEngine::generate` on a real model.
//!
//! Usage:
//!
//! ```bash
//! # With CUDA + real features (requires CUDA toolkit + model weights)
//! cargo run --release -p justapi-bench --bin justapi-gpu-bench \
//!     --features "cuda,real" \
//!     -- --model-path /path/to/model-dir/ --device cuda:0
//!
//! # TinyLlama GGUF example:
//! # 1. Download: curl -Lo /tmp/tinyllama-1.1b.q4_k_m.gguf \
//! #      https://huggingface.co/TheBloke/TinyLlama-1.1B-GGUF/resolve/main/...gguf
//! # 2. Run: cargo run ... -- --model-path /tmp/tinyllama-1.1b.q4_k_m.gguf
//! ```
//!
//! When no model is available or the `real` feature is missing, the binary
//! falls back to MockModel to exercise the scheduler path on CPU.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use anyhow::{Context, Result};
use clap::Parser;

use justapi_inference::{
    Engine, EngineDevice, KvBlockPool, SamplingParams, Scheduler, SchedulerConfig, SchedulerEngine,
};

/// GPU benchmark arguments.
#[derive(Parser, Debug)]
#[command(name = "justapi-gpu-bench", about = "GPU throughput benchmark for JustAPI inference")]
struct Args {
    /// Path to model directory (GGUF file or safetensors + config.json).
    #[arg(long, default_value = None)]
    model_path: Option<PathBuf>,

    /// Device to run on (cuda:0, cuda:1, cpu). Defaults to CPU when CUDA is
    /// not compiled in.
    #[arg(long, default_value_t = String::from("cpu"))]
    device: String,

    /// Model name to register in the Engine.
    #[arg(long, default_value = "bench-model")]
    model_name: String,

    /// Number of concurrent requests.
    #[arg(long, default_value_t = 4)]
    num_requests: usize,

    /// Prompt length (number of token ids).
    #[arg(long, default_value_t = 32)]
    prompt_len: usize,

    /// Maximum tokens to generate per request.
    #[arg(long, default_value_t = 128)]
    max_tokens: usize,

    /// Enable naive-path benchmark.
    #[arg(long)]
    bench_naive: bool,

    /// Enable scheduler-path benchmark.
    #[arg(long)]
    bench_scheduled: bool,
}

/// Per-request timing snapshot.
#[derive(Default)]
struct Timing {
    /// Wall clock for the entire batch.
    wall_ms: f64,
    /// Total tokens generated.
    total_tokens: u64,
    /// Time-to-first-token per request (ms).
    ttfts: Vec<f64>,
    /// Inter-token latencies (ms).
    itls: Vec<f64>,
}

fn report(name: &str, timing: &Timing) {
    let mut ttfts = timing.ttfts.clone();
    ttfts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut itls = timing.itls.clone();
    itls.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let tput = if timing.wall_ms > 0.0 {
        timing.total_tokens as f64 / (timing.wall_ms / 1000.0)
    } else {
        0.0
    };

    let ttft_p50 = ttfts.get(ttfts.len() / 2).copied().unwrap_or(0.0);
    let ttft_p99 = ttfts.get((ttfts.len() as f64 * 0.99) as usize).copied().unwrap_or(0.0);
    let itl_p50 = itls.get(itls.len() / 2).copied().unwrap_or(0.0);
    let itl_p99 = itls.get((itls.len() as f64 * 0.99) as usize).copied().unwrap_or(0.0);

    println!("### {name}");
    println!();
    println!("| Metric | Value |");
    println!("|---|---|");
    println!("| Total tokens | {} |", timing.total_tokens);
    println!("| Wall time (ms) | {:.2} |", timing.wall_ms);
    println!("| Throughput (tok/s) | {:.0} |", tput);
    println!("| TTFT p50 (ms) | {:.2} |", ttft_p50);
    println!("| TTFT p99 (ms) | {:.2} |", ttft_p99);
    println!("| ITL p50 (ms) | {:.2} |", itl_p50);
    println!("| ITL p99 (ms) | {:.2} |", itl_p99);
    println!();
}

fn main() -> Result<()> {
    let args = Args::parse();

    // --- Device setup ---
    let device = match args.device.as_str() {
        "cpu" => EngineDevice::Cpu,
        s if s.starts_with("cuda") => {
            let ordinal =
                s.strip_prefix("cuda:").and_then(|n| n.parse::<usize>().ok()).unwrap_or(0);
            EngineDevice::Cuda(ordinal)
        }
        s => anyhow::bail!("unsupported device: {s}"),
    };

    let engine = Arc::new(Engine::new(device).context("failed to create Engine")?);

    // --- Model loading ---
    if let Some(ref model_path) = args.model_path {
        #[cfg(feature = "real")]
        {
            if model_path.is_dir() || model_path.extension().map_or(false, |e| e == "gguf") {
                engine.load(&args.model_name, model_path).context("failed to load model")?;
                println!("Loaded model from: {}", model_path.display());
            } else {
                anyhow::bail!(
                    "model path must be a directory (safetensors) or .gguf file, got: {}",
                    model_path.display()
                );
            }
        }
        #[cfg(not(feature = "real"))]
        {
            let _ = model_path;
            anyhow::bail!(
                "Cannot load a real model without the `real` feature. \
                 Rebuild with: cargo build --features real"
            );
        }
    } else {
        engine.register_mock(&args.model_name);
        println!("No --model-path given; using MockModel (CPU, no GPU workload).");
    }

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .context("failed to build tokio runtime")?;

    // Build prompt tokens (0, 1, 2, ...).
    let prompt: Vec<u32> = (0..args.prompt_len as u32).collect();

    let params = SamplingParams {
        max_tokens: args.max_tokens,
        temperature: 0.0, // greedy for deterministic comparison
        ..Default::default()
    };

    // --- Scheduler setup ---
    let pool = KvBlockPool::new(4096);
    let config = SchedulerConfig { max_num_seqs: args.num_requests.max(1), ..Default::default() };
    let scheduler = Arc::new(Mutex::new(Scheduler::new(config, pool)));
    let se = Arc::new(SchedulerEngine::new(engine.clone(), scheduler));

    // ---- Naive path ----
    let all_default = !args.bench_naive && !args.bench_scheduled;
    if args.bench_naive || all_default {
        println!("--- Benchmark: naive Engine::generate ---");
        let timing = rt.block_on(bench_naive(
            &engine,
            &args.model_name,
            &prompt,
            &params,
            args.num_requests,
        ));
        report("Naive Engine", &timing);
    }

    // ---- Scheduled path ----
    if args.bench_scheduled || all_default {
        println!("--- Benchmark: SchedulerEngine::generate ---");
        let timing = rt.block_on(bench_scheduled(
            &se,
            &args.model_name,
            &prompt,
            &params,
            args.num_requests,
        ));
        report("SchedulerEngine", &timing);
    }

    Ok(())
}

/// Run `n` sequential requests through naive `Engine::generate`, measuring
/// per-request TTFT and token timings.
async fn bench_naive(
    engine: &Engine,
    model_name: &str,
    prompt: &[u32],
    params: &SamplingParams,
    n: usize,
) -> Timing {
    let mut timing = Timing::default();
    let batch_start = Instant::now();

    for _ in 0..n {
        let mut rx = engine.generate(model_name, prompt, params.clone()).unwrap();
        let mut req_first = true;
        let mut last_tok_time = Instant::now();

        while let Some(tok) = rx.recv().await {
            if req_first {
                timing.ttfts.push(last_tok_time.elapsed().as_secs_f64() * 1000.0);
                req_first = false;
                last_tok_time = Instant::now();
            }
            if tok.finish_reason.is_some() {
                break;
            }
            timing.itls.push(last_tok_time.elapsed().as_secs_f64() * 1000.0);
            last_tok_time = Instant::now();
            timing.total_tokens += 1;
        }
    }

    timing.wall_ms = batch_start.elapsed().as_secs_f64() * 1000.0;
    timing
}

/// Run `n` concurrent requests through scheduler-backed path, measuring
/// per-request TTFT and token timings.
async fn bench_scheduled(
    se: &SchedulerEngine,
    model_name: &str,
    prompt: &[u32],
    params: &SamplingParams,
    n: usize,
) -> Timing {
    let mut timing = Timing::default();
    let batch_start = Instant::now();

    // Admit all requests up front (continuous batching interleaves them).
    let mut receivers = Vec::with_capacity(n);
    for _ in 0..n {
        receivers.push(se.generate(model_name, prompt, params.clone()).unwrap());
    }

    for rx in receivers.iter_mut() {
        let mut req_first = true;
        let mut last_tok_time = Instant::now();

        while let Some(tok) = rx.recv().await {
            if req_first {
                timing.ttfts.push(last_tok_time.elapsed().as_secs_f64() * 1000.0);
                req_first = false;
                last_tok_time = Instant::now();
            }
            if tok.finish_reason.is_some() {
                break;
            }
            timing.itls.push(last_tok_time.elapsed().as_secs_f64() * 1000.0);
            last_tok_time = Instant::now();
            timing.total_tokens += 1;
        }
    }

    timing.wall_ms = batch_start.elapsed().as_secs_f64() * 1000.0;
    timing
}
