#!/usr/bin/env bash
# scripts/sanitize.sh — fast memory-safety gate (replaces the full-suite Miri run).
#
# Why not full Miri: Miri interprets every instruction (100-1000x slower); the
# full justapi-core suite takes 20+ minutes and most of it is safe Rust. Only
# ONE production file has `unsafe` (memory.rs, 3 mentions, with SAFETY
# comments). The practical gate is:
#
#   1. AddressSanitizer on the full core suite (near-native speed, catches
#      use-after-free / buffer overflow / leaks).
#   2. A targeted Miri run on just memory.rs's unsafe (via its #[cfg(miri)]
#      test) — covers the only UB-sensitive code path.
#
# Prereqs: rustup nightly with miri + rust-src (added automatically below).
# ASan needs `-Zbuild-std` (builds std instrumented) because of proc-macro
# crates (time) that break under blanket RUSTFLAGS sanitizer.
#
# Usage:
#   bash scripts/sanitize.sh            # ASan + targeted miri
#   bash scripts/sanitize.sh --asan     # ASan only
#   bash scripts/sanitize.sh --miri     # targeted miri only

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$ROOT"

MODE="${1:-all}"
ASAN_TARGET_DIR="${ASAN_TARGET_DIR:-/tmp/justapi-asan-target}"
MIRI_TARGET_DIR="${MIRI_TARGET_DIR:-/tmp/justapi-miri-target}"
ASAN_LOG="$(mktemp /tmp/justapi-asan.XXXXXX.log)"
MIRI_LOG="$(mktemp /tmp/justapi-miri.XXXXXX.log)"

# -Zbuild-std needs the std source for the target. Idempotent; harmless if
# already installed.
rustup component add rust-src --toolchain nightly >/dev/null 2>&1 || true

run_step() {
    local name="$1" log="$2"; shift 2
    echo "[sanitize] $name..."
    if ! "$@" >"$log" 2>&1; then
        echo "[sanitize] $name FAILED (exit $?) — last 25 lines:" >&2
        tail -25 "$log" >&2
        exit 1
    fi
    # Surface the summary line if present, then confirm pass.
    grep -E "test result:" "$log" | tail -1 || true
    echo "[sanitize] $name: PASS"
}

if [[ "$MODE" == "all" || "$MODE" == "--asan" ]]; then
    # Exclude benchmark-gate tests: they assert on wall-clock timing and fail
    # under sanitizer instrumentation (~2x slowdown). Benchmarks are for
    # release builds; sanitizers are for memory safety.
    run_step "AddressSanitizer on justapi-core (lib, features=db)" "$ASAN_LOG" \
        env PATH="$HOME/.cargo/bin:$PATH" \
        CARGO_TARGET_DIR="$ASAN_TARGET_DIR" \
        RUSTFLAGS="-Zsanitizer=address" \
        rustup run nightly cargo test -p justapi-core --lib --features db \
            -Zbuild-std --target x86_64-unknown-linux-gnu \
            -- --skip bench_
fi

if [[ "$MODE" == "all" || "$MODE" == "--miri" ]]; then
    # The #[cfg(miri)] test in memory.rs exercises the arena's
    # from_utf8_unchecked under stacked-borrows provenance rules.
    run_step "Miri on the only unsafe module (memory.rs, targeted)" "$MIRI_LOG" \
        env PATH="$HOME/.cargo/bin:$PATH" \
        CARGO_TARGET_DIR="$MIRI_TARGET_DIR" \
        MIRIFLAGS="-Zmiri-disable-isolation" \
        rustup run nightly cargo miri test -p justapi-core --lib memory
fi

echo "[sanitize] done — memory-safety gate green"
