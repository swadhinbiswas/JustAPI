"""Fill the expense database with ~6 months of realistic demo data.

Usage::

    python seed.py            # seed only if the table is empty
    python seed.py --force    # wipe existing rows first

Deterministic: same data every run (random.seed(7)).
"""

from __future__ import annotations

import random
import sys
from datetime import date, timedelta

from app import app  # noqa: E402  (connects to EXPENSES_DB / ./expenses.db)

MERCHANTS: dict[str, list[tuple[str, float, float]]] = {
    # category: [(description, min, max), ...]
    "food": [
        ("Groceries", 18, 95),
        ("Lunch with team", 9, 24),
        ("Coffee beans", 8, 19),
        ("Dinner out", 22, 68),
    ],
    "transport": [
        ("Fuel", 25, 70),
        ("Train ticket", 4, 22),
        ("Ride share", 6, 28),
    ],
    "rent": [("Monthly rent", 950, 950)],
    "utilities": [
        ("Electricity bill", 38, 96),
        ("Internet", 35, 35),
        ("Water bill", 14, 30),
    ],
    "entertainment": [
        ("Streaming subscription", 9, 16),
        ("Cinema", 12, 34),
        ("Concert ticket", 35, 110),
    ],
    "health": [
        ("Pharmacy", 7, 45),
        ("Gym membership", 29, 29),
        ("Doctor visit", 30, 90),
    ],
    "shopping": [
        ("Clothes", 20, 140),
        ("Home supplies", 12, 75),
        ("Electronics accessory", 15, 120),
    ],
}

MONTHLY_FIXED = ["rent", "utilities"]  # roughly once per month


def main() -> int:
    force = "--force" in sys.argv
    app.db  # touch to connect + apply schema

    existing = app.db.query("SELECT COUNT(*) AS n FROM expenses")[0]["n"]
    if existing and not force:
        print(f"database already has {existing} expenses — use --force to reseed")
        return 0
    if force and existing:
        app.db.execute("DELETE FROM expenses")
        app.db.execute("DELETE FROM audit_log")
        print(f"cleared {existing} existing expenses")

    random.seed(7)
    today = date.today()
    start = today - timedelta(days=180)
    rows: list[tuple] = []

    day = start
    while day <= today:
        # 0-3 discretionary expenses per day
        for _ in range(random.randint(0, 3)):
            category = random.choice(
                ["food", "food", "transport", "entertainment", "health", "shopping"]
            )
            description, low, high = random.choice(MERCHANTS[category])
            rows.append(
                (
                    description,
                    round(random.uniform(low, high), 2),
                    category,
                    day.isoformat(),
                )
            )
        # Fixed monthly bills on the 1st and 15th
        if day.day in (1, 15):
            for category in MONTHLY_FIXED:
                description, low, high = MERCHANTS[category][0]
                amount = low if low == high else round(random.uniform(low, high), 2)
                rows.append((description, amount, category, day.isoformat()))
        day += timedelta(days=1)

    for description, amount, category, spent_on in rows:
        app.db.execute(
            "INSERT INTO expenses (description, amount, category, spent_on) "
            "VALUES (?, ?, ?, ?)",
            [description, amount, category, spent_on],
        )
    # No expense id for a bulk seed: omit the column rather than binding NULL
    # (a NULL parameter is typed as text and Postgres rejects it for an
    # integer column).
    app.db.execute("INSERT INTO audit_log (action) VALUES (?)", ["seed"])

    summary = app.db.query("SELECT COUNT(*) AS count, SUM(amount) AS total FROM expenses")[0]
    print(f"seeded {summary['count']} expenses, total {round(summary['total'], 2)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
