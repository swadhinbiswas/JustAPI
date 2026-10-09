# =============================================================================
# Stage 1: Build the Rust CLI binary and Python wheel
# =============================================================================
FROM rust:1.97-bookworm AS builder

RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev pkg-config python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/

RUN python3 -m venv /opt/venv && \
    . /opt/venv/bin/activate && \
    pip install maturin uv
ENV PATH="/opt/venv/bin:$PATH"

RUN cargo build --release -p justapi-cli --features tls,compression,ws
RUN strip target/release/justapi-cli

RUN maturin build -m crates/justapi-py/Cargo.toml --release -o wheels/

# =============================================================================
# Stage 2: Minimal runtime image with Python
# =============================================================================
FROM python:3.12-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# The Rust CLI is installed under its real binary name so the Python console
# script (`justapi`) can delegate create/check/openapi to it. Naming it
# `justapi` here would be overwritten by the pip console script (and silently
# break `justapi serve` for Python apps — the Rust binary cannot load them).
COPY --from=builder /app/target/release/justapi-cli /usr/local/bin/justapi-cli
COPY --from=builder /app/wheels /tmp/wheels

RUN pip install uv && uv pip install --system /tmp/wheels/*.whl && rm -rf /tmp/wheels

# demo_app.py uses pydantic for request validation
RUN pip install --no-cache-dir "pydantic>=2.0" "jinja2>=3.0"

COPY demo_app.py /app/main.py

EXPOSE 8080

# Security: drop root privileges
RUN useradd -m -u 1000 -s /bin/bash justapi
USER justapi

# Health check — verifies the server is responding on /live
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://localhost:8080/live', timeout=5)"]

# `justapi serve` runs the app in the working directory (main.py above). The
# Rust CLI (`justapi-cli`) is installed alongside for create/check/openapi.
# Kubernetes/Helm overrides `args` (e.g. `serve --addr 0.0.0.0:8080`) — with
# this ENTRYPOINT those args run through the `justapi` console script.
# To run your own app instead: mount it as /app/main.py, or override the
# entrypoint (`docker run --entrypoint python image app.py`).
ENTRYPOINT ["justapi"]
CMD ["serve", "--addr", "0.0.0.0:8080"]
