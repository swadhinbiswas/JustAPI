---
title: Docker & Docker Compose
description: Containerize your JustAPI application for development and production.
---

## Multi-Stage Dockerfile

The repository includes a production-ready multi-stage Dockerfile:

```dockerfile
# Stage 1: Build Rust CLI binary and Python wheel
FROM rust:1.75-bookworm AS builder

RUN apt-get update && apt-get install -y \
    libssl-dev pkg-config python3 python3-pip python3-venv

WORKDIR /app
COPY Cargo.toml Cargo.lock ./
COPY crates/ crates/
COPY python/ python/

RUN python3 -m venv /opt/venv
ENV PATH="/opt/venv/bin:$PATH"
RUN pip install maturin
RUN cargo build --release -p justapi-cli --features tls,compression
RUN maturin build -m crates/justapi-py/Cargo.toml --release -o wheels/

# Stage 2: Minimal runtime
FROM python:3.12-slim-bookworm
RUN apt-get update && apt-get install -y ca-certificates libssl3

WORKDIR /app
COPY --from=builder /app/target/release/justapi /usr/local/bin/justapi
COPY --from=builder /app/wheels /tmp/wheels
RUN pip install --no-cache-dir /tmp/wheels/*.whl

COPY . .
EXPOSE 8080
CMD ["python", "app.py"]
```

### Build the Image

```bash
docker build -t my-justapi-app .
docker run -p 8080:8080 my-justapi-app
```

## Docker Compose (Development)

```yaml
version: "3.9"
services:
  justapi:
    build: .
    ports:
      - "8080:8080"
    environment:
      - RUST_LOG=info
      - DATABASE_URL=postgres://justapi:justapi@db:5432/justapi
    depends_on:
      db:
        condition: service_healthy

  db:
    image: postgres:16-alpine
    environment:
      POSTGRES_USER: justapi
      POSTGRES_PASSWORD: justapi
      POSTGRES_DB: justapi
    healthcheck:
      test: ["CMD-SHELL", "pg_isready -U justapi"]
      interval: 5s
      timeout: 5s
      retries: 5
```

```bash
docker compose up -d
```

## OpenTelemetry Stack

Generated projects include `docker-compose.otel.yml`:

```yaml
services:
  jaeger:
    image: jaegertracing/all-in-one:latest
    ports:
      - "16686:16686"  # UI
      - "4318:4318"    # OTLP HTTP
```

## Production Image Optimizations

1. **Use `python:3.12-slim`** — Small image (~120 MB)
2. **Set `panic = "abort"`** — Already configured in Cargo.toml
3. **Run as non-root** — Use `USER nobody` or create an app user
4. **Add HEALTHCHECK** — Use `/health` endpoint
5. **Use `.dockerignore`** — Exclude target/, .git/, __pycache__/

### Example Production Dockerfile Snippet

```dockerfile
FROM python:3.12-slim-bookworm
RUN adduser --disabled-password --gecos '' appuser
WORKDIR /app
COPY --from=builder /app/target/release/justapi /usr/local/bin/justapi
COPY --from=builder /app/wheels /tmp/wheels
RUN pip install --no-cache-dir /tmp/wheels/*.whl && rm -rf /tmp/wheels
COPY app.py .
USER appuser
EXPOSE 8080
HEALTHCHECK --interval=30s --timeout=3s CMD python -c "import urllib.request; urllib.request.urlopen('http://localhost:8080/health')"
CMD ["python", "app.py"]
```

## See Also

- [Kubernetes / Helm](/deployment/kubernetes-helm/) — Orchestrate with K8s
- [Production Checklist](/deployment/production-checklist/) — Production readiness
- [Docker Compose File](https://github.com/swadhinbiswas/JustAPI/blob/main/docker-compose.yml)
