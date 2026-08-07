#!/usr/bin/env bash
# scripts/publish.sh — local build + verify (the RELEASE path is CI).
#
# PRIMARY RELEASE PATH (no token): push a `v*` tag → .github/workflows/wheels.yml
# builds the 7-platform matrix and publishes to PyPI via OIDC trusted
# publishing (see the setup note at the bottom of wheels.yml). This script is
# the local pre-release verification: portable manylinux + musllinux builds
# (maturin --zig), twine check, fresh-venv install smoke.
#
# Legacy token path (kept for emergencies): MATURIN_PYPI_TOKEN=... bash scripts/publish.sh --upload
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

echo "[publish] building wheels for: ${PLATFORMS[*]} + free-threaded 3.14t"
mkdir -p "$WHEELS_DIR"
for target in "${PLATFORMS[@]}"; do
    echo "[publish] building $target..."
    (cd "$ROOT/crates/justapi-py" && maturin build --release --target "$target" --zig --out "$WHEELS_DIR")
done

# Free-threaded CPython 3.14t: no abi3 (limited API can't be free-threaded).
echo "[publish] building free-threaded x86_64 (3.14t, no abi3)..."
(cd "$ROOT/crates/justapi-py" && maturin build --release --target x86_64-unknown-linux-gnu \
    --interpreter python3.14t --no-default-features --features mail,http3,orjson \
    --zig --out "$WHEELS_DIR") || echo "[publish] WARN: free-threaded wheel skipped (needs python3.14t)"

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
    echo "    (preferred) git tag v2.0.8 && git push origin v2.0.8   # CI publishes via OIDC"
    echo "    (emergency) MATURIN_PYPI_TOKEN=... bash scripts/publish.sh --upload"
fi
