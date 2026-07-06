---
name: benchmark-harness
description: >
  Use when running or interpreting a benchmark, adding a new workload, or
  comparing justapi against Uvicorn/Granian/Hypercorn. Covers the exact
  invocation, hardware assumptions, and how to distinguish real improvements
  from noise.
---

# Benchmark Harness

## When to use

- Before and after every phase to record numbers in `BENCHMARKS.md`.
- When deciding whether a p99 regression is real (>5% threshold).
- Adding a new workload type (e.g., DB-round-trip-simulated, file serving).
- Comparing against a competing server (Uvicorn, Granian, Hypercorn).
- Validating a performance claim before writing it in docs or PLAN.md.

## Setup

### Required tools

```bash
# Primary HTTP load generator (outputs p50/p95/p99 natively)
cargo install oha

# Allocation profiling (pick one)
# dhat: integrated via cargo, no external install needed
# heaptrack: `sudo pacman -S heaptrack` (Arch) or `apt install heaptrack`

# CPU profiling
cargo install flamegraph
```

### Hardware fixture

Record once in `BENCHMARKS.md` and reuse for all runs:
- CPU model + core count
- RAM size
- Kernel version
- Whether virtualized (bare metal vs VM vs container)
- OS distribution

## Protocol

### 1. Warm up

Minimum 5 seconds of traffic before measuring. This ensures:
- JIT compilation (Python side) is complete
- Connection pools are established
- OS caches are warm

### 2. Run parameters

```bash
# Standard benchmark: 30s duration, 100 concurrent connections
oha -z 30s -c 100 --latency-correction --disable-keepalive http://127.0.0.1:8080/hello

# JSON echo: POST with nested payload
oha -z 30s -c 100 -m POST \
    -H "Content-Type: application/json" \
    -d '{"user":{"name":"test","id":42},"items":[1,2,3],"meta":{"version":"1.0"}}' \
    http://127.0.0.1:8080/echo
```

### 3. Minimum runs

**3 runs per workload**, report the distribution:
- p50, p95, p99 latency
- Requests/sec (throughput)
- Peak RSS (via `/proc/<pid>/status` → VmHWM, or `time -v`)

### 4. Record in BENCHMARKS.md

Append a new section — **never overwrite** old numbers. Include:
- Date
- Tool version and exact command line
- All three runs' numbers
- The median or best-of-three, clearly labeled

### 5. Regression check

A p99 regression >5% vs. the previous phase's recorded numbers **fails the
benchmark gate** unless `DECISIONS.md` documents an accepted trade-off with
the rationale.

## Workloads

| Name | Route | Method | Description |
|---|---|---|---|
| hello-world | `/hello` | GET | Static JSON response — routing + serialization floor |
| json-echo | `/echo` | POST | Nested JSON payload echoed back — parser + serializer |
| db-sim | `/db` | GET | `tokio::time::sleep(5ms)` in handler — simulates DB I/O |

## Measuring peak RSS

```bash
# Option 1: /proc (during run)
cat /proc/<pid>/status | grep VmHWM

# Option 2: GNU time wrapper
/usr/bin/time -v ./target/release/justapi serve 2>&1 | grep "Maximum resident"
```

## Gotchas

- **Single-run noise:** One run proves nothing. Three+ runs establish a
  distribution. If variance is >10%, investigate before drawing conclusions.
- **Comparing across hardware:** Absolute numbers are hardware-dependent.
  Only compare numbers from the same fixture.
- **Tool version drift:** Record the exact tool version in `BENCHMARKS.md`.
- **Background load:** Ensure no competing CPU/memory load. CI runners
  are noisy — prefer dedicated hardware for release benchmarks.
- **Keep-alive vs no-keepalive:** Document which mode was used. Keep-alive
  measures steady-state; no-keepalive includes connection setup cost.
- **Python server workers:** Match worker count to justapi's thread count
  for fair comparison (e.g., `uvicorn --workers 4`).

## References

- oha: https://github.com/hatoo/oha
- wrk: https://github.com/wg/wrk
- `BENCHMARKS.md` in project root
- `DECISIONS.md` — ADR-006 for tooling choice rationale
