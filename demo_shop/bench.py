"""Launch the demo_shop server, wait for it to bind, then run a load test
with `oha` against a representative mix of endpoints. Results are written to
/tmp/demo_bench.json and printed.

Run:
    .venv/bin/python demo_shop/bench.py
"""
import subprocess
import sys
import time
import os

HERE = os.path.dirname(os.path.abspath(__file__))
APP = os.path.join(HERE, "app.py")
HOST, PORT = "127.0.0.1", 8099
BASE = f"http://{HOST}:{PORT}"

# 1) Start the server as a subprocess.
proc = subprocess.Popen(
    [sys.executable, APP, "--host", HOST, "--port", str(PORT)],
    stdout=open("/tmp/demo_srv.log", "w"),
    stderr=subprocess.STDOUT,
)

# 2) Wait for the port to accept connections.
up = False
for _ in range(60):
    time.sleep(0.5)
    try:
        r = subprocess.run(["curl", "-s", "-m", "2", f"{BASE}/shop/health"],
                           capture_output=True, text=True)
        if r.returncode == 0 and r.stdout.strip():
            up = True
            break
    except Exception:
        pass
if not up:
    print("SERVER FAILED TO START")
    print(open("/tmp/demo_srv.log").read()[:2000])
    proc.terminate()
    sys.exit(1)

print("server up")

# 3) Probe each endpoint once to confirm health.
import json
endpoints = {
    "shop/health": "GET",
    "products?size=20": "GET",
    "products?category=bed_bath_table&size=10": "GET",
    "categories": "GET",
    "sellers?size=10": "GET",
    "reviews?size=10": "GET",
    "reviews/summary": "GET",
    "geolocation/states": "GET",
    "analytics/sales-by-category": "GET",
    "analytics/top-sellers?limit=10": "GET",
    "analytics/delivery-performance": "GET",
    "analytics/monthly-revenue": "GET",
    "orders?size=10": "GET",
    "search?q=sao": "GET",
}
for ep in endpoints:
    r = subprocess.run(["curl", "-s", "-m", "5", "-o", "/dev/null", "-w", "%{http_code}",
                       f"{BASE}/{ep}"], capture_output=True, text=True)
    print(f"  probe {ep}: {r.stdout}")

# 4) Load test with oha: 15s, 64 concurrent, across a few representative routes.
results = {}
for name, ep in [
    ("products_list", "/products?size=20"),
    ("product_detail", "/products/0a1b2c3d4e5f00001"),  # will 404 fast; use a real one below
    ("analytics_sales", "/analytics/sales-by-category"),
    ("sellers", "/sellers?size=20"),
]:
    pass

# Use a real product id for detail benchmark.
real_pid = subprocess.run(
    ["curl", "-s", "-m", "5", f"{BASE}/products?size=1"],
    capture_output=True, text=True).stdout
try:
    pid = json.loads(real_pid)["items"][0]["product_id"]
except Exception:
    pid = "x"

load_targets = {
    "products_list": f"{BASE}/products?size=20",
    "analytics_sales": f"{BASE}/analytics/sales-by-category",
    "sellers": f"{BASE}/sellers?size=20",
    "order_detail": f"{BASE}/orders/" + json.loads(
        subprocess.run(["curl", "-s", "-m", "5", f"{BASE}/orders?size=1"],
                       capture_output=True, text=True).stdout)["items"][0]["order_id"],
    "product_detail": f"{BASE}/products/{pid}",
}

for name, url in load_targets.items():
    print(f"\n=== oha {name} ===")
    r = subprocess.run(
        ["oha", "-z", "15s", "-c", "64", url],
        capture_output=True, text=True,
    )
    out = r.stdout
    results[name] = out
    # Print the summary lines.
    for line in out.splitlines():
        if any(k in line for k in ["Requests/sec", "Slowest", "Fastest", "p99", "p75", "p90", "p50", "DNS", "Status", "Requests", "Duration", "connections"]):
            print("  " + line)

# 5) Tear down.
proc.terminate()
try:
    proc.wait(timeout=10)
except subprocess.TimeoutExpired:
    proc.kill()

with open("/tmp/demo_bench.txt", "w") as f:
    f.write(json.dumps(results, indent=2))
print("\nDONE — results in /tmp/demo_bench.txt")
