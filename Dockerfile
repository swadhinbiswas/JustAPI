# =============================================================================
# Stage 1: Build the Rust CLI binary and Python wheel
# =============================================================================
FROM rust:1.75-bookworm AS builder

# Install dependencies including python for building maturin wheels
RUN apt-get update && apt-get install -y --no-install-recommends \
    libssl-dev pkg-config python3 python3-pip python3-venv \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Cache dependencies
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY python/ python/

RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"

RUN pip install maturin

# Build the Rust CLI
RUN cargo build --release -p justapi-cli --features tls,compression
RUN strip target/release/justapi

# Build the Python wheel
RUN maturin build -m crates/justapi-py/Cargo.toml --release -o wheels/

# =============================================================================
# Stage 2: Minimal runtime image with Python
# =============================================================================
FROM python:3.12-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

# Copy CLI binary
COPY --from=builder /app/target/release/justapi /usr/local/bin/justapi

# Copy and install the wheel
COPY --from=builder /app/wheels /tmp/wheels
RUN pip install --no-cache-dir /tmp/wheels/*.whl && rm -rf /tmp/wheels

COPY demo_app.py /app/

EXPOSE 8080

ENTRYPOINT ["python", "demo_app.py"]
