# Order Platform — production container.
#
# Build (from this directory):
#   docker build -t order-platform .
# Or install a local wheel before release:
#   docker build --build-arg JUSTAPI_SPEC=./justapi-2.1.0-cp311-abi3-manylinux_2_34_x86_64.whl -t order-platform .
#
# Run:
#   docker run --rm -p 8000:8000 -v order-data:/data order-platform
#   docker exec -u appuser <container> python seed.py   # demo customers/products
#
# The database lives in the /data volume. Point PLATFORM_DB at Postgres for a
# managed deployment (same code).

FROM python:3.12-slim AS builder

ARG JUSTAPI_SPEC=justapi==2.1.0
COPY . /ctx
WORKDIR /ctx
RUN python -m venv /venv \
    && /venv/bin/pip install --no-cache-dir --upgrade pip \
    && /venv/bin/pip install --no-cache-dir "$JUSTAPI_SPEC" "pydantic>=2.0"

FROM python:3.12-slim AS runtime

ENV PYTHONUNBUFFERED=1 \
    PATH="/venv/bin:$PATH" \
    PLATFORM_DB=/data/platform.db \
    PORT=8000 \
    WORKERS=4

COPY --from=builder /venv /venv

WORKDIR /app
COPY app.py seed.py ./

RUN useradd --system --uid 10001 --create-home appuser \
    && mkdir -p /data \
    && chown appuser:appuser /data
USER appuser

EXPOSE 8000
VOLUME ["/data"]

# /ready probes the database; /live only checks the process.
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request, sys; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8000/live', timeout=2).status == 200 else 1)"

CMD ["sh", "-c", "exec justapi serve --addr 0.0.0.0:${PORT} --workers ${WORKERS}"]
