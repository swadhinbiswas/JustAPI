#!/usr/bin/env python3
"""Extract the p99 latency (in seconds) from oha's human-readable output.

Usage:
    oha -z 15s -c 100 URL | python benchmarks/extract_p99.py

oha prints a "Response time distribution" block whose units are chosen
per-run (ns / µs / ms / s), so the size of the number alone is not enough —
the unit suffix must be honoured. Prints the p99 in **seconds** so callers
can compare ratios directly. Exits non-zero when the line is missing
(e.g. aborted run), so CI fails loudly instead of comparing garbage.
"""

import re
import sys

_UNIT_SECONDS = {
    "ns": 1e-9,
    "us": 1e-6,
    "µs": 1e-6,
    "ms": 1e-3,
    "s": 1.0,
}

# e.g. "  99.00% in 2.4002 ms"
_P99_RE = re.compile(r"^\s*99\.00%\s+in\s+([\d.]+)\s*(ns|us|µs|ms|s)\s*$", re.M)


def main() -> int:
    text = sys.stdin.read()
    match = _P99_RE.search(text)
    if not match:
        print("extract_p99: no '99.00% in ...' line found in oha output", file=sys.stderr)
        return 1
    value, unit = match.group(1), match.group(2)
    seconds = float(value) * _UNIT_SECONDS[unit]
    print(f"{seconds:.9f}")
    return 0


if __name__ == "__main__":
    sys.exit(main())
