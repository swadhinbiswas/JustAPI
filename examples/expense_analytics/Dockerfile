# Expense Tracker with Analytics — production container.
#
# Build (from this directory):
#   docker build -t expense-analytics .
#
# The `JUSTAPI_SPEC` build arg lets you pin a version or install a local
# wheel, e.g. before the release is on PyPI:
#   docker build --build-arg JUSTAPI_SPEC=./justapi-2.1.0-cp311-abi3-manylinux_2_34_x86_64.whl -t expense-analytics .
#
# Run:
#   docker run --rm -p 8000:8000 -v expense-data:/data expense-analytics
#   curl http://127.0.0.1:8000/analytics/summary
#
# The SQLite database lives in the `/data` volume (`EXPENSES_DB`), so data
# survives container restarts. Use `docker compose up` for the ready-made
# setup (see docker-compose.yml).

# --- builder: install the framework into an isolated venv -------------------
FROM python:3.12-slim AS builder

ARG JUSTAPI_SPEC=justapi==2.1.0
# The app uses Pydantic models; the framework ships them as the `pydantic`
# extra (`pip install "justapi[pydantic]"`). Installed explicitly so the
# same line works when JUSTAPI_SPEC is a local wheel path.
COPY . /ctx
WORKDIR /ctx
RUN python -m venv /venv \
    && /venv/bin/pip install --no-cache-dir --upgrade pip \
    && /venv/bin/pip install --no-cache-dir "$JUSTAPI_SPEC" "pydantic>=2.0"

# --- runtime: app files + venv, non-root, healthcheck -----------------------
FROM python:3.12-slim AS runtime

ENV PYTHONUNBUFFERED=1 \
    PATH="/venv/bin:$PATH" \
    EXPENSES_DB=/data/expenses.db \
    PORT=8000 \
    WORKERS=4

COPY --from=builder /venv /venv

WORKDIR /app
COPY app.py seed.py ./

# Non-root runtime user; the DB volume is the only writable path.
RUN useradd --system --uid 10001 --create-home appuser \
    && mkdir -p /data \
    && chown appuser:appuser /data
USER appuser

EXPOSE 8000
VOLUME ["/data"]

# Liveness: /live answers as soon as the server accepts requests.
# (If you override PORT, update this URL to match.)
HEALTHCHECK --interval=30s --timeout=3s --start-period=5s --retries=3 \
    CMD python -c "import urllib.request, sys; sys.exit(0 if urllib.request.urlopen('http://127.0.0.1:8000/live', timeout=2).status == 200 else 1)"

# 4 prefork workers share the port via SO_REUSEPORT; SQLite WAL keeps
# concurrent readers/writers safe. Tune WORKERS to the CPU limit.
CMD ["sh", "-c", "exec justapi serve --addr 0.0.0.0:${PORT} --workers ${WORKERS}"]
