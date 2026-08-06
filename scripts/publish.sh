#!/usr/bin/env bash
# scripts/publish.sh — build + verify + publish the justapi PyPI wheels.
#
# The only missing piece for a real release is the PyPI token
# (MATURIN_PYPI_TOKEN / TWINE_PASSWORD). Everything else — portable
# manylinux + musllinux builds for x86_64 + aarch64 (via maturin --zig),
# twine check, fresh-venv install smoke — is done here so
# `bash scripts/publish.sh` is the single release command.
#
# Prereqs: maturin (with --zig; requires zig on PATH), rustup with the
# target stds installed (aarch64-unknown-linux-gnu, aarch64-unknown-linux-musl,
# x86_64-unknown-linux-musl), python3.
#
# Usage:
#   bash scripts/publish.sh            # build + verify (safe, no upload)
#   MATURIN_PYPI_TOKEN=... bash scripts/publish.sh --upload

set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
WHEELS_DIR="$ROOT/target/wheels-user"
UPLOAD="${1:-}"

PLATFORMS=(
    "x86_64-unknown-linux-gnu"
    "aarch64-unknown-linux-gnu"
    "x86_64-unknown-linux-musl"
    "aarch64-unknown-linux-musl"
)

echo "[publish] building wheels for: ${PLATFORMS[*]}"
mkdir -p "$WHEELS_DIR"
for target in "${PLATFORMS[@]}"; do
    echo "[publish] building $target..."
    (cd "$ROOT/crates/justapi-py" && maturin build --release --target "$target" --zig --out "$WHEELS_DIR")
done

echo "[publish] wheels:"
ls -la "$WHEELS_DIR"/*.whl

echo "[publish] twine check (all wheels)..."
python3 -m venv /tmp/justapi_publish_venv
/tmp/justapi_publish_venv/bin/pip install --quiet twine
/tmp/justapi_publish_venv/bin/python -m twine check "$WHEELS_DIR"/*.whl

echo "[publish] fresh-venv install smoke (x86_64 manylinux)..."
WHEEL="$(ls "$WHEELS_DIR"/*manylinux*x86_64*.whl | head -1)"
/tmp/justapi_publish_venv/bin/pip install --quiet "$WHEEL"
/tmp/justapi_publish_venv/bin/python -c "
from justapi import JustAPIApp, Depends, Security, Schema
app = JustAPIApp()
@app.get('/')
def root(): return {'ok': True}
from justapi import JustAPITestClient
r = JustAPITestClient(app).get('/')
assert r['status'] == 200, r
print('fresh-venv smoke: OK')
"

if [[ "$UPLOAD" == "--upload" ]]; then
    if [[ -z "${MATURIN_PYPI_TOKEN:-}" && -z "${TWINE_PASSWORD:-}" ]]; then
        echo "[publish] ERROR: --upload requires MATURIN_PYPI_TOKEN or TWINE_PASSWORD" >&2
        exit 1
    fi
    echo "[publish] uploading to PyPI..."
    /tmp/justapi_publish_venv/bin/python -m twine upload --skip-existing "$WHEELS_DIR"/*.whl
    echo "[publish] DONE — justapi is on PyPI (4 platforms)"
else
    echo "[publish] build + verify OK. To release:"
    echo "    MATURIN_PYPI_TOKEN=... bash scripts/publish.sh --upload"
fi
