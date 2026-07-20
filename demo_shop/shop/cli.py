"""shop.app — CLI entrypoint for the MVP shop.

Run:  python -m shop --port 8080
(or)  python shop/app.py --port 8080
"""
from __future__ import annotations

import argparse

from . import app


def main():
    parser = argparse.ArgumentParser(description="demo_shop MVP e-commerce API")
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=8080)
    args = parser.parse_args()
    app.run(f"{args.host}:{args.port}")


if __name__ == "__main__":
    main()
