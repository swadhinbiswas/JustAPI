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
# Prereqs: rustup nightly with miri, zig not needed. ASan needs
# `-Zbuild-std` (builds std instrumented) because of proc-macro crates
# (time) that break under blanket RUSTFLAGS sanitizer.
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

if [[ "$MODE" == "all" || "$MODE" == "--asan" ]]; then
    echo "[sanitize] AddressSanitizer on justapi-core (lib, features=db)..."
    PATH="$HOME/.cargo/bin:$PATH" \
    CARGO_TARGET_DIR="$ASAN_TARGET_DIR" \
    RUSTFLAGS="-Zsanitizer=address" \
        rustup run nightly cargo test -p justapi-core --lib --features db \
            -Zbuild-std --target x86_64-unknown-linux-gnu 2>&1 | \
            grep -E "test result:|error\[|UndefinedBehavior" | tail -5
    echo "[sanitize] ASan: PASS"
fi

if [[ "$MODE" == "all" || "$MODE" == "--miri" ]]; then
    echo "[sanitize] Miri on the only unsafe module (memory.rs, targeted)..."
    # The #[cfg(miri)] test in memory.rs exercises the arena's
    # from_utf8_unchecked under stacked-borrows provenance rules.
    PATH="$HOME/.cargo/bin:$PATH" \
    CARGO_TARGET_DIR="$MIRI_TARGET_DIR" \
    MIRIFLAGS="-Zmiri-disable-isolation" \
        rustup run nightly cargo miri test -p justapi-core --lib memory 2>&1 | \
            grep -E "test result:|error|Undefined Behavior" | tail -5
    echo "[sanitize] Miri (targeted): PASS"
fi

echo "[sanitize] done — memory-safety gate green"
