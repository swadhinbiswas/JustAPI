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

RUN cargo build --release -p justapi-cli --features tls,compression
RUN strip target/release/justapi

RUN maturin build -m crates/justapi-py/Cargo.toml --release -o wheels/

# =============================================================================
# Stage 2: Minimal runtime image with Python
# =============================================================================
FROM python:3.12-slim-bookworm

RUN apt-get update && apt-get install -y --no-install-recommends \
    ca-certificates libssl3 \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/justapi /usr/local/bin/justapi
COPY --from=builder /app/wheels /tmp/wheels

RUN pip install uv && uv pip install --system /tmp/wheels/*.whl && rm -rf /tmp/wheels

# demo_app.py uses pydantic for request validation
RUN pip install --no-cache-dir pydantic>=2.0

COPY demo_app.py /app/

EXPOSE 8080

# Security: drop root privileges
RUN useradd -m -u 1000 -s /bin/bash justapi
USER justapi

# Health check — verifies the server is responding on /live
HEALTHCHECK --interval=30s --timeout=5s --start-period=10s --retries=3 \
    CMD ["python", "-c", "import urllib.request; urllib.request.urlopen('http://localhost:8080/live')"]

# demo_app.py ends with app.run("127.0.0.1:8080") — run the Python app.
ENTRYPOINT ["python", "demo_app.py"]
