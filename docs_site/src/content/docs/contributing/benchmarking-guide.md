---
title: Benchmarking Guide
description: How to run and interpret JustAPI benchmarks.
---

## Setup

```bash
# Install benchmarking tools
cargo install oha

# Build the benchmark harness
cargo build --release -p justapi-bench
```

## Running Benchmarks

### Standard Workloads

```bash
# Build a baseline workload
python benchmarks/workloads_justapi.py &
oha -z 30s -c 100 http://localhost:8000/hello
oha -z 30s -c 100 http://localhost:8000/echo
```

### CRUD Benchmarks (DB-backed)

```bash
# JustAPI vs FastAPI vs Robyn with SQLite
bash benchmarks/run_crud_bench.sh
```

### Serialization Benchmarks

```bash
cargo run --release -p justapi-bench --bin justapi-bench
```

## Benchmark Protocol

1. **Warm up**: Minimum 5 seconds before recording
2. **Run**: 30 seconds, 100 concurrent connections
3. **Repeat**: Minimum 3 runs per workload
4. **Record**: p50, p95, p99 latency, RPS, peak RSS

## Key Workloads

| Name | Route | Method | Description |
|---|---|---|---|
| hello-world | `/hello` | GET | Static JSON response |
| json-echo | `/echo` | POST | Nested JSON payload echoed |
| db-sim | `/db` | GET | Simulated DB query (5ms sleep) |
| native | `/native` | POST | Native fast path (724k+ RPS) |
| crud-read | `/items` | GET | SQLite SELECT |
| crud-write | `/items` | POST | SQLite INSERT |

## Regression Gate

A p99 regression >5% compared to the previous baseline **fails the benchmark gate**. The CI workflow (`bench.yml`) enforces this:

```
PR_RPS >= 0.95 * MAIN_RPS  # Must pass
```

## Interpreting Results

```
Requests/sec:    701,234
Latency:
  p50:          0.07 ms
  p95:          0.12 ms
  p99:          0.19 ms
```

- **RPS**: Higher is better
- **p50**: Typical latency (median)
- **p99**: Worst-case latency (tail)
- **Difference between p50 and p99**: Indicates latency consistency

## Hardware Fixture

Record the exact hardware when benchmarking:

```
CPU:  13th Gen Intel Core i5-13600K
RAM:  31 GiB DDR5
OS:   CachyOS Linux
Kernel: 6.x
```

Comparisons are only valid on the same hardware.

## Adding New Benchmarks

1. Add your workload to `benchmarks/workloads_*.py`
2. Add a new workload entry in `BENCHMARKS.md`
3. Run 3+ iterations and record all numbers
4. Do **not** overwrite historical data — append

## See Also

- [Performance Tuning](/advanced/performance-tuning/) — Optimize your app
- [BENCHMARKS.md](https://github.com/swadhinbiswas/JustAPI/blob/main/BENCHMARKS.md)
- [Benchmark Harness Skill](/skills/benchmark-harness/SKILL.md)
