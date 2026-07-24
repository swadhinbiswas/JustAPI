---
title: Development Setup
description: Set up your local environment for JustAPI development — the Rust-powered FastAPI alternative. Build, test, and contribute to the fastest Python web framework.
keywords: [JustAPI, FastAPI alternative, development setup, Rust Python, PyO3, maturin, contribute to JustAPI]
---

## Prerequisites

- Python 3.11+
- Rust 1.85+ (install via [rustup](https://rustup.rs/))
- Git

## Clone the Repository

```bash
git clone https://github.com/swadhinbiswas/JustAPI.git
cd JustAPI
```

## Python Environment

### Using standard venv + pip

```bash
python -m venv .venv
source .venv/bin/activate
pip install maturin pydantic
```

### Using uv (recommended for speed)

```bash
uv venv
source .venv/bin/activate
uv pip install maturin pydantic
```

## Build the Rust Extension

```bash
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```

For debug builds (faster compile, slower runtime):

```bash
maturin develop --manifest-path crates/justapi-py/Cargo.toml
```

## Install CLI

```bash
cargo install --path crates/justapi-cli
```

## Install All Dependencies

```bash
# Rust
cargo build --workspace

# Python (with pip)
pip install -e ".[dev]"

# Python (with uv)
uv pip install -e ".[dev]"
```

## Editor Setup

Recommended extensions:

- **rust-analyzer** — Rust language support
- **pyright** or **pylance** — Python language support
- **maturin** — Python↔Rust integration

## Running Tests

```bash
# Rust tests
cargo test --workspace

# Python tests
pytest

# Linting
cargo clippy --workspace --tests -- -D warnings
cargo fmt --check
```

## Next Steps

- [Coding Standards](/contributing/coding-standards/) — Code conventions and guidelines
- [Testing Guide](/contributing/testing-guide/) — How to write tests
- [Benchmarking Guide](/contributing/benchmarking-guide/) — Performance testing
