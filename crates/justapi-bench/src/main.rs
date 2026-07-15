use std::time::Instant;

use serde::Serialize;

#[derive(Serialize)]
struct NestedPayload {
    user: User,
    items: Vec<u32>,
    meta: Metadata,
}

#[derive(Serialize)]
struct User {
    name: String,
    id: u32,
}

#[derive(Serialize)]
struct Metadata {
    version: String,
    timestamp: u64,
}

fn payload() -> NestedPayload {
    NestedPayload {
        user: User { name: "test".into(), id: 42 },
        items: vec![1, 2, 3],
        meta: Metadata { version: "1.0".into(), timestamp: 1700000000 },
    }
}

fn bench<F>(name: &str, f: F, iterations: usize)
where
    F: Fn(),
{
    for _ in 0..1000 {
        f();
    }

    let start = Instant::now();
    for _ in 0..iterations {
        f();
    }
    let elapsed = start.elapsed();
    let avg_ns = elapsed.as_nanos() as f64 / iterations as f64;
    let ops_per_sec = (iterations as f64 / elapsed.as_secs_f64()) as u64;

    println!(
        "| {} | {:.0} ns/op | {:.0} ops/sec | {} iterations in {:.2?} |",
        name, avg_ns, ops_per_sec, iterations, elapsed
    );
}

fn main() {
    println!();
    println!("## Serialization benchmarks");
    println!();
    println!("Payload: nested JSON object (user+items+meta), ~60 bytes");
    println!();
    println!("| Backend | Latency | Throughput | Details |");
    println!("|---|---|---|---|");

    let p = payload();
    let iterations = 1_000_000;

    bench(
        "justapi_core::serialize (default=serde_json)",
        || {
            use justapi_core::serialize::to_json_string;
            let _ = to_json_string(&p).unwrap();
        },
        iterations,
    );

    #[cfg(feature = "simd-json")]
    bench(
        "justapi_core::serialize (simd-json feature)",
        || {
            use justapi_core::serialize::to_json_string;
            let _ = to_json_string(&p).unwrap();
        },
        iterations,
    );

    println!();
    println!("---");
    println!();
    println!("Hardware: 13th Gen Intel Core i5-13600K, DDR5");
    println!();
}
