//! Inference-engine scheduler benchmark — structural proof-of-mechanism on the
//! CPU fixture.
//!
//! No GPU is available in CI, so this does **not** measure wall-clock LLM
//! tokens/sec. Instead it drives the *real* JustAPI schedulers
//! ([`PdScheduler`] for disaggregated prefill/decode, [`Scheduler`] for a
//! collocated pool) with a parameterized synthetic GPU-cost model and reports
//! the scheduling metrics that define LLM-serving quality:
//!
//! - **TTFT** (time to first token) — prefill latency experienced by a request.
//! - **ITL** (inter-token latency) — decode-step spacing (streaming smoothness).
//! - **Throughput** — total generated tokens / wall time.
//!
//! ## Cost + parallelism model
//!
//! Prefill cost scales with prompt tokens processed in a step (compute-bound);
//! decode cost is a single fixed step regardless of batch size
//! (memory-bandwidth-bound) — the standard vLLM/SGLang assumption.
//!
//! The two topologies differ in *hardware*:
//! - **Disaggregated** runs prefill and decode on **independent pools** (separate
//!   GPUs), so their timelines advance in parallel. Decode ITL is therefore
//!   decoupled from prefill — it stays uniform at `decode_us` even while prefill
//!   is busy.
//! - **Collocated** shares one pool, so each step serialises prefill-then-decode
//!   on the same budget; decode ITL is inflated by whatever prefill work shares
//!   the step.
//!
//! We model this with two independent virtual clocks (disaggregated) vs one
//! shared clock (collocated). This is a faithful structural model of the
//! scheduler topology, not a wall-clock GPU claim.

use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use justapi_inference::{
    Engine, EngineDevice, KvBlockPool, NewRequest, PdScheduler, SamplingParams, Scheduler,
    SchedulerConfig, SchedulerEngine,
};

/// Benchmark parameters.
#[derive(Clone, Copy)]
struct BenchParams {
    /// Number of concurrent requests submitted at t=0 (burst load).
    num_requests: usize,
    /// Base prompt length; actual length varies per request to force prefill/decode
    /// overlap (so the collocated scheduler's ITL degradation is visible).
    prompt_len: usize,
    /// Max new tokens to generate per request.
    max_tokens: usize,
    /// Synthetic prefill cost: microseconds per prompt token in a prefill step.
    prefill_us_per_token: u64,
    /// Synthetic decode cost: microseconds for one batched decode step.
    decode_us_per_token: u64,
    /// Chunked-prefill size (None = whole prompt at once).
    chunked_prefill_size: Option<usize>,
}

/// Collected scheduling metrics for one run (times in milliseconds).
#[derive(Default)]
struct Metrics {
    /// Per-request time-to-first-token.
    ttfts: Vec<f64>,
    /// Per-decode-step inter-token latency.
    itls: Vec<f64>,
    /// Total generated (decode) tokens.
    total_tokens: u64,
    /// Wall-clock duration (virtual).
    wall_ms: f64,
    /// Tokens transferred prefill→decode pool (disaggregated only).
    transferred_tokens: usize,
}

fn make_request(id: u64, prompt_len: usize, max_tokens: usize) -> NewRequest {
    NewRequest {
        id,
        prompt: (0..prompt_len as u32).collect(),
        sampling_params: SamplingParams {
            max_tokens,
            ..Default::default()
        },
        prefix_cached_tokens: 0,
        cached_blocks: Vec::new(),
    }
}

fn percentile(sorted: &[f64], p: f64) -> f64 {
    if sorted.is_empty() {
        return 0.0;
    }
    let rank = (p * (sorted.len() as f64 - 1.0)).round() as usize;
    sorted[rank.min(sorted.len() - 1)]
}

/// Run the **disaggregated** configuration: independent prefill and decode pools
/// modelled with parallel virtual clocks.
fn run_disaggregated(p: &BenchParams) -> Metrics {
    let mut pd = PdScheduler::new(
        SchedulerConfig {
            max_num_seqs: p.num_requests.max(1),
            max_num_batched_tokens: 8192,
            max_seq_len: 8192,
            chunked_prefill_size: p.chunked_prefill_size,
        },
        KvBlockPool::new(8192),
        SchedulerConfig {
            max_num_seqs: p.num_requests.max(1),
            max_num_batched_tokens: 8192,
            max_seq_len: 8192,
            chunked_prefill_size: p.chunked_prefill_size,
        },
        KvBlockPool::new(8192),
    );
    for i in 0..p.num_requests as u64 {
        // Vary prompt length so sequences finish prefill at different steps and
        // prefill/decode overlap.
        let pl = p.prompt_len + ((i as usize) % 5) * 48;
        pd.add_request(make_request(i, pl, p.max_tokens));
    }

    // Two independent timelines (parallel prefill + decode GPUs).
    let mut prefill_clock = 0.0_f64; // µs
    let mut decode_clock = 0.0_f64; // µs
    let mut m = Metrics::default();
    let mut decoded_once: HashSet<u64> = HashSet::new();
    let mut prev_decode_clock: Option<f64> = None;

    loop {
        if pd.is_idle() {
            break;
        }

        // ---- prefill pool ----
        let sched = pd.schedule_prefill();
        let p_cost: f64 = sched.prefill.iter().map(|s| s.tokens.len()).sum::<usize>() as f64
            * p.prefill_us_per_token as f64;
        prefill_clock += p_cost;
        for step in &sched.prefill {
            pd.on_prefill_complete(step.seq_id, 1);
        }
        m.transferred_tokens += pd.transfer_completed();

        // ---- decode pool (runs in parallel on its own GPU) ----
        let sched = pd.schedule_decode();
        let d_cost: f64 = if sched.decode.is_empty() {
            0.0
        } else {
            p.decode_us_per_token as f64
        };
        decode_clock += d_cost;
        if !sched.decode.is_empty() {
            // Global virtual time = the later of the two pool timelines.
            let now = prefill_clock.max(decode_clock);
            for &seq_id in &sched.decode {
                if decoded_once.insert(seq_id) {
                    m.ttfts.push(now / 1000.0);
                }
                pd.on_decode_complete(seq_id, 1);
            }
            m.total_tokens += sched.decode.len() as u64;
            // ITL is measured on the decode pool's own timeline (independent).
            if let Some(prev) = prev_decode_clock {
                m.itls.push((decode_clock - prev) / 1000.0);
            }
            prev_decode_clock = Some(decode_clock);
        }
    }

    m.wall_ms = decode_clock.max(prefill_clock) / 1000.0;
    m
}

/// Run the **collocated** configuration: one scheduler, one shared virtual clock
/// (prefill and decode serialised on the same pool/budget).
fn run_collocated(p: &BenchParams) -> Metrics {
    let mut sched = Scheduler::new(
        SchedulerConfig {
            max_num_seqs: p.num_requests.max(1),
            max_num_batched_tokens: 8192,
            max_seq_len: 8192,
            chunked_prefill_size: p.chunked_prefill_size,
        },
        KvBlockPool::new(8192),
    );
    for i in 0..p.num_requests as u64 {
        let pl = p.prompt_len + ((i as usize) % 5) * 48;
        sched.add_request(make_request(i, pl, p.max_tokens));
    }

    let mut t = 0.0_f64; // µs, single shared clock
    let mut m = Metrics::default();
    let mut decoded_once: HashSet<u64> = HashSet::new();
    let mut prev_t: Option<f64> = None;

    loop {
        if sched.is_idle() {
            break;
        }
        let s = sched.schedule();

        // Shared pool: prefill and decode serialised in the same step.
        let mut cost = 0.0_f64;
        for step in &s.prefill {
            cost += (step.tokens.len() as f64) * p.prefill_us_per_token as f64;
        }
        if !s.decode.is_empty() {
            cost += p.decode_us_per_token as f64;
        }
        t += cost;

        for step in &s.prefill {
            sched.on_step_complete(step.seq_id, 1);
        }
        for &seq_id in &s.decode {
            if decoded_once.insert(seq_id) {
                m.ttfts.push(t / 1000.0);
            }
            sched.on_step_complete(seq_id, 1);
        }
        m.total_tokens += s.decode.len() as u64;
        if !s.decode.is_empty() {
            if let Some(prev) = prev_t {
                m.itls.push((t - prev) / 1000.0);
            }
            prev_t = Some(t);
        }
    }

    m.wall_ms = t / 1000.0;
    m
}

fn report(name: &str, m: &Metrics, p: &BenchParams) {
    let mut ttfts = m.ttfts.clone();
    ttfts.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut itls = m.itls.clone();
    itls.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let tput = if m.wall_ms > 0.0 {
        m.total_tokens as f64 / (m.wall_ms / 1000.0)
    } else {
        0.0
    };

    println!("### {name}");
    println!();
    println!("| Metric | p50 | p99 |");
    println!("|---|---|---|");
    println!(
        "| TTFT (ms) | {:.2} | {:.2} |",
        percentile(&ttfts, 0.5),
        percentile(&ttfts, 0.99)
    );
    println!(
        "| ITL (ms) | {:.2} | {:.2} |",
        percentile(&itls, 0.5),
        percentile(&itls, 0.99)
    );
    println!("| Throughput (tok/s) | {:.0} | |", tput);
    println!("| Total tokens | {} | |", m.total_tokens);
    println!("| Wall time (ms) | {:.2} | |", m.wall_ms);
    if m.transferred_tokens > 0 {
        println!("| Transferred tokens (P→D) | {} | |", m.transferred_tokens);
    }
    println!();
    println!(
        "Config: {} requests, prompt≈{}-{} tok, max_tokens={}, prefill={}µs/tok, decode={}µs/step, chunk={:?}",
        p.num_requests,
        p.prompt_len,
        p.prompt_len + 4 * 48,
        p.max_tokens,
        p.prefill_us_per_token,
        p.decode_us_per_token,
        p.chunked_prefill_size
    );
    println!();
}

// ---------------------------------------------------------------------------
// Real wall-clock throughput (CPU fixture, MockModel)
//
// The structural benchmark above models GPU cost synthetically. This section
// additionally drives the *real* generation path (naive `Engine::generate` vs
// scheduler-backed `SchedulerEngine::generate`) and measures actual token
// throughput on the CPU fixture. This quantifies the scheduler's per-token
// overhead (thread hop + lock + sampling) — not LLM tokens/sec, which needs a
// GPU.
// ---------------------------------------------------------------------------

const REAL_MODEL: &str = "mock";

/// Run `n` requests through the naive `Engine::generate` path and return
/// (total_tokens, wall_ms).
fn bench_naive(n: usize, prompt_len: usize, max_tokens: usize) -> (u64, f64) {
    let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
    engine.register_mock(REAL_MODEL);
    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let prompt: Vec<u32> = (0..prompt_len as u32).collect();
    let params = SamplingParams {
        max_tokens,
        ..Default::default()
    };

    let start = Instant::now();
    let total: u64 = rt.block_on(async {
        let mut total = 0u64;
        for _ in 0..n {
            let mut rx = engine
                .generate(REAL_MODEL, &prompt, params.clone())
                .unwrap();
            while let Some(tok) = rx.recv().await {
                if tok.finish_reason.is_some() {
                    break;
                }
                total += 1;
            }
        }
        total
    });
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    (total, wall_ms)
}

/// Run `n` requests through the scheduler-backed `SchedulerEngine` path and
/// return (total_tokens, wall_ms).
fn bench_scheduled(n: usize, prompt_len: usize, max_tokens: usize) -> (u64, f64) {
    let engine = Arc::new(Engine::new(EngineDevice::Cpu).unwrap());
    engine.register_mock(REAL_MODEL);
    let pool = KvBlockPool::new(4096);
    let config = SchedulerConfig {
        max_num_seqs: n.max(1),
        ..Default::default()
    };
    let scheduler = Arc::new(Mutex::new(Scheduler::new(config, pool)));
    let se = Arc::new(SchedulerEngine::new(engine.clone(), scheduler));

    let rt = tokio::runtime::Builder::new_current_thread()
        .build()
        .unwrap();

    let prompt: Vec<u32> = (0..prompt_len as u32).collect();
    let params = SamplingParams {
        max_tokens,
        ..Default::default()
    };

    let start = Instant::now();
    let total: u64 = rt.block_on(async {
        let mut total = 0u64;
        // All requests are admitted up front; the persistent scheduler loop
        // interleaves them (continuous batching).
        let mut receivers = Vec::new();
        for _ in 0..n {
            receivers.push(se.generate(REAL_MODEL, &prompt, params.clone()).unwrap());
        }
        for rx in receivers.iter_mut() {
            while let Some(tok) = rx.recv().await {
                if tok.finish_reason.is_some() {
                    break;
                }
                total += 1;
            }
        }
        total
    });
    let wall_ms = start.elapsed().as_secs_f64() * 1000.0;
    (total, wall_ms)
}

fn main() {
    let params = BenchParams {
        num_requests: 16,
        prompt_len: 64,
        max_tokens: 64,
        prefill_us_per_token: 5,
        decode_us_per_token: 200,
        chunked_prefill_size: Some(16),
    };

    println!("# JustAPI inference scheduler benchmark — disaggregated P/D vs collocated");
    println!();
    println!(
        "Hardware: 13th Gen Intel Core i5-13600K (20 threads), CachyOS. \
         Structural run — GPU cost modeled synthetically (no GPU in CI). \
         Disaggregated = parallel prefill/decode timelines; collocated = shared timeline."
    );
    println!();

    let coll = run_collocated(&params);
    let disagg = run_disaggregated(&params);

    report(
        "Collocated (single pool, prefill+decode serialised)",
        &coll,
        &params,
    );
    report(
        "Disaggregated (independent prefill/decode pools)",
        &disagg,
        &params,
    );

    let mut coll_itl = coll.itls.clone();
    coll_itl.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mut disagg_itl = disagg.itls.clone();
    disagg_itl.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let coll_p99 = percentile(&coll_itl, 0.99);
    let disagg_p99 = percentile(&disagg_itl, 0.99);
    let itl_improvement = if disagg_p99 > 0.0 {
        coll_p99 / disagg_p99
    } else {
        1.0
    };
    let tput_improvement = if coll.wall_ms > 0.0 {
        disagg.wall_ms / coll.wall_ms
    } else {
        1.0
    };

    println!("---");
    println!();
    println!(
        "**ITL p99:** collocated {:.2} ms → disaggregated {:.2} ms ({:.2}x tighter)",
        coll_p99, disagg_p99, itl_improvement
    );
    println!(
        "**Wall time:** collocated {:.2} ms → disaggregated {:.2} ms ({:.2}x faster, parallel pools)",
        coll.wall_ms, disagg.wall_ms, tput_improvement
    );
    println!();

    // ---- Real wall-clock throughput comparison ----
    println!("---");
    println!();
    println!("# Real wall-clock throughput (CPU fixture, MockModel)");
    println!();
    println!(
        "Measures actual generation latency on the CPU — quantifies scheduler \
         per-token overhead (thread hop + lock contention + sampling). \
         All models are MockModel (instant forward pass)."
    );
    println!();

    let n_requests_real = 8;
    let prompt_len_real = 8;
    let max_tokens_real = 16;
    let (naive_tokens, naive_ms) = bench_naive(n_requests_real, prompt_len_real, max_tokens_real);
    let (sched_tokens, sched_ms) =
        bench_scheduled(n_requests_real, prompt_len_real, max_tokens_real);

    let naive_tput = (naive_tokens as f64) / (naive_ms / 1000.0);
    let sched_tput = (sched_tokens as f64) / (sched_ms / 1000.0);
    let overhead = if naive_tput > 0.0 {
        sched_tput / naive_tput * 100.0
    } else {
        0.0
    };

    println!("| Path | Total tokens | Wall (ms) | Throughput (tok/s) | Overhead vs naive |");
    println!("|---|---|---|---|---|");
    println!("| Naive Engine | {naive_tokens} | {naive_ms:.2} | {naive_tput:.0} | — |");
    println!(
        "| SchedulerEngine | {sched_tokens} | {sched_ms:.2} | {sched_tput:.0} | Scheduler {overhead:.1}% of naive |"
    );
    println!();
    println!(
        "Config: {n_requests_real} requests, {prompt_len_real} prompt tokens, {max_tokens_real} max_tokens each."
    );
    println!();
}
