"""In-process load test for the demo_shop API.

The framework's `app.run()` currently DEADLOCKS on a configured SQLite database
(see demo_shop/README, Finding #4) so we cannot benchmark through the TCP
server yet. This harness instead drives the real handlers + real Rust DB pool
through the framework's AsyncTestClient (same handler dispatch, same DB pool,
minus the raw TCP/HTTP layer) under heavy concurrent load, to surface
handler/DB/serialization/GIL bottlenecks.

Run:
    .venv/bin/python demo_shop/bench_async.py
"""
import asyncio
import json
import os
import sys
import time

HERE = os.path.dirname(os.path.abspath(__file__))
sys.path.insert(0, os.path.dirname(HERE))
from justapi.testing import AsyncTestClient  # noqa: E402
import demo_shop.app as shop  # noqa: E402

DB_URL = f"sqlite://{shop.DB_PATH}"

# Endpoints to hammer (representative read mix of a real shop).
ENDPOINTS = [
    "/shop/health",
    "/products?size=20",
    "/products?category=bed_bath_table&size=10",
    "/categories",
    "/sellers?size=10",
    "/reviews?size=10",
    "/reviews/summary",
    "/geolocation/states",
    "/analytics/sales-by-category",
    "/analytics/top-sellers?limit=10",
    "/analytics/delivery-performance",
    "/analytics/monthly-revenue",
    "/orders?size=10",
    "/search?q=sao",
]


async def worker(client, ep, n, counter, lat):
    for _ in range(n):
        t0 = time.perf_counter()
        try:
            resp = await client.get(ep)
            dt = (time.perf_counter() - t0) * 1000.0
            lat.append(dt)
            counter[resp.get("status", 0) == 200 and "ok" or "err"] += 1
        except Exception:
            counter["err"] += 1
            lat.append((time.perf_counter() - t0) * 1000.0)


async def run_load(concurrency, per_worker):
    async with AsyncTestClient(shop.app, database=DB_URL) as c:
        # prime one request per endpoint so plans are warm
        for ep in ENDPOINTS:
            await c.get(ep)
        counter = {"ok": 0, "err": 0}
        lat = []
        tasks = []
        for ep in ENDPOINTS:
            for _ in range(concurrency):
                tasks.append(asyncio.create_task(worker(c, ep, per_worker, counter, lat)))
        t0 = time.perf_counter()
        await asyncio.gather(*tasks)
        wall = time.perf_counter() - t0
        return counter, lat, wall


def pct(vals, p):
    if not vals:
        return 0.0
    s = sorted(vals)
    k = max(0, min(len(s) - 1, int(p / 100.0 * len(s))))
    return s[k]


def main():
    import argparse
    ap = argparse.ArgumentParser()
    ap.add_argument("--concurrency", type=int, default=32)
    ap.add_argument("--per-worker", type=int, default=50)
    args = ap.parse_args()

    counter, lat, wall = asyncio.run(run_load(args.concurrency, args.per_worker))
    total = counter["ok"] + counter["err"]
    print("\n=== demo_shop in-process load (AsyncTestClient) ===")
    print(f"concurrency={args.concurrency} per_worker={args.per_worker} "
          f"total_reqs={total} wall={wall:.2f}s")
    print(f"throughput = {total / wall:.0f} req/s")
    print(f"ok={counter['ok']} err={counter['err']}")
    if lat:
        print(f"latency ms: p50={pct(lat,50):.2f} p90={pct(lat,90):.2f} "
              f"p99={pct(lat,99):.2f} max={max(lat):.2f}")
    # Save raw for records.
    with open("/tmp/demo_bench_async.json", "w") as f:
        json.dump({"concurrency": args.concurrency, "per_worker": args.per_worker,
                   "total": total, "wall_s": wall,
                   "throughput_rps": total / wall, "ok": counter["ok"],
                   "err": counter["err"],
                   "p50_ms": pct(lat, 50), "p90_ms": pct(lat, 90),
                   "p99_ms": pct(lat, 99)}, f, indent=2)


if __name__ == "__main__":
    main()
