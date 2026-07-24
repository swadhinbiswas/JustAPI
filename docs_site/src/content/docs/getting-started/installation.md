---
title: Installation Guide
description: Install JustAPI — the fastest FastAPI alternative — via pip, uv, cargo, or build from source. JustAPI is a Rust-powered Python web framework with 700k+ RPS.
keywords: [JustAPI, FastAPI alternative, install JustAPI, pip install, uv install, Python web framework, Rust Python, FastAPI replacement]
---

## Prerequisites

- **Python:** 3.11, 3.12, 3.13, or 3.14 (including 3.13t/3.14t free-threaded builds)
- **Rust:** 1.85+ (only required if building from source)

## Install via pip

Install the official pre-compiled wheel package from PyPI:

```bash
pip install justapi
```

To install optional features (Pydantic v2, Jinja2, XML support):

```bash
pip install "justapi[full]"
```

## Install via uv

[uv](https://docs.astral.sh/uv/) is a fast Python package manager written in Rust. Install JustAPI with uv:

```bash
uv pip install justapi
```

With optional features:

```bash
uv pip install "justapi[full]"
```

## Run without Installing (via uvx)

[uvx](https://docs.astral.sh/uv/guides/tools/) lets you run JustAPI CLI commands without a permanent install:

```bash
# Scaffold a new project (no install needed)
uvx justapi create my_app

# Run the dev server
uvx justapi serve --reload
```

This is ideal for CI pipelines, quick prototyping, and ephemeral environments.

### Verify Installation

```bash
python -c "import justapi; print(justapi.__version__)"
# Output: 2.0.0
```

## Install CLI Tooling (`justapi-cli`)

The `justapi` CLI includes the project scaffolder, dev server with hot-reload, and database migration runner.

```bash
# Install from source (requires Rust 1.85+)
cargo install justapi-cli

# Verify
justapi --version
# Output: justapi 2.0.0
```

## Docker

Pull the official Docker image:

```bash
docker pull ghcr.io/justapi/justapi:latest
```

Or use the multi-stage `Dockerfile` in the repository root for custom builds.

## Build from Source

Clone the repository and build the native PyO3 bindings with `maturin`:

```bash
git clone https://github.com/swadhinbiswas/JustAPI.git
cd JustAPI

# Create a virtual environment
python -m venv .venv
source .venv/bin/activate

# Build the Rust extension
maturin develop --release --manifest-path crates/justapi-py/Cargo.toml

# Install Python dependencies
pip install pydantic

# Build the CLI (optional)
cargo install --path crates/justapi-cli
```

### Build with Feature Flags

```bash
# Enable TLS, compression, and OpenTelemetry support
maturin develop --release -m crates/justapi-py/Cargo.toml --features tls,compression,opentelemetry
```

## Platform-Specific Notes

### Linux

Pre-built wheels are available for `manylinux_2_28` (x86_64 and ARM64) and `musllinux_1_2` (Alpine).

Required system packages:

```bash
# Debian/Ubuntu
apt-get install libssl-dev pkg-config python3-dev

# Alpine
apk add openssl-dev pkgconfig python3-dev rust cargo
```

### macOS

Pre-built wheels available for x86_64 and ARM64 (Apple Silicon).

```bash
# Ensure you have Xcode Command Line Tools
xcode-select --install
```

### Windows

Pre-built wheels available for x64. When building from source, install Visual Studio Build Tools with C++ support.

## Troubleshooting

### `pip install` fails with build error

Ensure you have Rust installed. If you don't need to build from source, try the pre-built wheel:

```bash
pip install --only-binary=justapi justapi
```

### `maturin develop` fails

```bash
# Ensure you're in a virtual environment
python -m venv .venv
source .venv/bin/activate

# Update pip and maturin
pip install --upgrade pip maturin
```

### ImportError: No module named `_justapi`

The Rust extension wasn't built. Run `maturin develop --release` from the repository root.

## Next Steps

- [First Steps](/getting-started/first-steps/) — Build your first API endpoint in 2 minutes
- [CLI Project Scaffolder](/getting-started/cli-scaffolder/) — Generate complete project templates
