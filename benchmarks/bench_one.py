"""bench_one.py — run a single oha workload and print rps/p50/p99 (JSON parsed).

Usage:
    python benchmarks/bench_one.py <method> <url> [body-json]
Prints a single TSV line:  <rps>\t<p50_ms>\t<p99_ms>
Exits non-zero on parse failure (caller should handle).
"""
import json
import subprocess
import sys

METHOD = sys.argv[1]
URL = sys.argv[2]
BODY = sys.argv[3] if len(sys.argv) > 3 else None
DUR = "5s"
CONC = "20"
WARM = "2s"

base = ["oha", "-z", DUR, "-c", CONC, "--output-format", "json"]
if METHOD in ("POST", "PUT", "DELETE"):
    base += ["-m", METHOD, "-H", "Content-Type: application/json"]
    if BODY:
        base += ["-d", BODY]
base += [URL]

# warmup
subprocess.run(
    ["oha", "-z", WARM, "-c", CONC] + (["-m", METHOD, "-H", "Content-Type: application/json", "-d", BODY] if (METHOD in ("POST", "PUT", "DELETE") and BODY) else []) + [URL],
    stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
)

out = subprocess.run(base, stdout=subprocess.PIPE, stderr=subprocess.DEVNULL, text=True)
try:
    d = json.loads(out.stdout)
    rps = round(d["summary"]["requestsPerSec"], 1)
    p50 = d["latencyPercentiles"].get("p50")
    p99 = d["latencyPercentiles"].get("p99")
    p50 = round(p50 / 1e6, 2) if p50 else 0.0
    p99 = round(p99 / 1e6, 2) if p99 else 0.0
    sr = round(d["summary"].get("successRate", 1.0) * 100, 1)
    print(f"{rps}\t{p50}\t{p99}\t{sr}")
except Exception as e:
    print(f"NA\tNA\tNA\tNA", file=sys.stderr)
    print(f"parse-error: {e}", file=sys.stderr)
    sys.exit(1)
