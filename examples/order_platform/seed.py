"""Create the demo customers and products.

    python seed.py            # idempotent
    python seed.py --force    # wipe orders/products/customers and reseed

Demo API keys (development only — rotate for anything real):
    admin-dev-key        -> admin
    customer-dev-key     -> Ada Lovelace
    customer-2-dev-key   -> Grace Hopper
"""

from __future__ import annotations

import sys

from app import app, seed_database


def main() -> int:
    created = seed_database(app.db, force="--force" in sys.argv)
    counts = app.db.query(
        "SELECT (SELECT COUNT(*) FROM customers) AS customers, "
        "(SELECT COUNT(*) FROM products) AS products"
    )[0]
    print(
        f"seeded {created['customers']} customers, {created['products']} products "
        f"(total: {counts['customers']} customers, {counts['products']} products)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
