---
title: Installation Guide
description: How to install JustAPI via pip, cargo, or building from source.
---

## Prerequisites

* **Python:** 3.11, 3.12, 3.13, or 3.14 (including 3.13t/3.14t free-threaded builds)
* **Rust:** 1.85+ (only required if building from source)

## Install via pip

Install the official pre-compiled wheel package from PyPI:

```bash
pip install justapi
```

To install optional features (such as Pydantic v2 and Jinja2 support):

```bash
pip install "justapi[full]"
```

## Install CLI Tooling (`justapi-cli`)

Install the native Rust project generator and dev server CLI via Cargo:

```bash
cargo install justapi-cli
```

## Build from Source

To compile the native PyO3 bindings locally with `maturin`:

```bash
git clone https://github.com/swadhinbiswas/JustAPI.git
cd JustAPI
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml
```
