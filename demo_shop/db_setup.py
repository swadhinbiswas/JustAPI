"""db_setup — owned application database for the MVP shop.

The Olist `olist.sqlite` is a large *read-only* historical dataset. To build a
real MVP with create/update/delete operations we keep a separate, *owned*
SQLite database (`shop.db`) that we control. On first run it is created and
seeded from Olist (categories, sellers, customers, and a product catalog with
managed stock/price), then all mutations happen against `shop.db`.

Run directly to (re)build/seed:  python db_setup.py
"""
from __future__ import annotations

import os
import sqlite3
import time

HERE = os.path.dirname(os.path.abspath(__file__))
OLIST_DB = os.environ.get("DEMO_SHOP_OLIST", os.path.join(HERE, "olist.sqlite"))
SHOP_DB = os.environ.get("DEMO_SHOP_DB", os.path.join(HERE, "shop.db"))

SCHEMA = """
PRAGMA foreign_keys = ON;

CREATE TABLE IF NOT EXISTS categories (
    id          INTEGER PRIMARY KEY,
    slug        TEXT UNIQUE NOT NULL,
    name_pt     TEXT NOT NULL,
    name_en     TEXT
);

CREATE TABLE IF NOT EXISTS sellers (
    id          TEXT PRIMARY KEY,            -- original Olist seller_id
    city        TEXT,
    state       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS customers (
    id          TEXT PRIMARY KEY,            -- original Olist customer_id
    unique_id   TEXT,
    city        TEXT,
    state       TEXT,
    email       TEXT,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS products (
    id          TEXT PRIMARY KEY,            -- original Olist product_id
    category_id INTEGER REFERENCES categories(id),
    name_pt     TEXT,
    weight_g    REAL,
    length_cm   REAL,
    height_cm   REAL,
    width_cm    REAL,
    price       REAL NOT NULL DEFAULT 0,     -- managed selling price
    stock       INTEGER NOT NULL DEFAULT 0,  -- managed inventory
    active      INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS carts (
    id          TEXT PRIMARY KEY,            -- public cart token
    customer_id TEXT REFERENCES customers(id),
    created_at  TEXT NOT NULL DEFAULT (datetime('now')),
    updated_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS cart_items (
    cart_id     TEXT NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    product_id  TEXT NOT NULL REFERENCES products(id),
    quantity    INTEGER NOT NULL DEFAULT 1,
    PRIMARY KEY (cart_id, product_id)
);

CREATE TABLE IF NOT EXISTS orders (
    id              TEXT PRIMARY KEY,        -- public order token
    customer_id     TEXT NOT NULL REFERENCES customers(id),
    status          TEXT NOT NULL DEFAULT 'pending',  -- pending|paid|shipped|delivered|cancelled
    total_amount    REAL NOT NULL DEFAULT 0,
    shipping_amount REAL NOT NULL DEFAULT 0,
    created_at      TEXT NOT NULL DEFAULT (datetime('now')),
    paid_at         TEXT,
    shipped_at      TEXT,
    delivered_at    TEXT
);

CREATE TABLE IF NOT EXISTS order_items (
    order_id    TEXT NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id  TEXT NOT NULL REFERENCES products(id),
    seller_id   TEXT REFERENCES sellers(id),
    quantity    INTEGER NOT NULL DEFAULT 1,
    unit_price  REAL NOT NULL,
    freight     REAL NOT NULL DEFAULT 0,
    PRIMARY KEY (order_id, product_id)
);

CREATE TABLE IF NOT EXISTS payments (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id    TEXT NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    method      TEXT NOT NULL,              -- credit_card|voucher|debit_card|boleto
    installments INTEGER NOT NULL DEFAULT 1,
    amount      REAL NOT NULL,
    created_at  TEXT NOT NULL DEFAULT (datetime('now'))
);

CREATE TABLE IF NOT EXISTS reviews (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    order_id        TEXT NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    product_id      TEXT REFERENCES products(id),
    customer_id     TEXT REFERENCES customers(id),
    score           INTEGER NOT NULL CHECK (score BETWEEN 1 AND 5),
    title           TEXT,
    comment         TEXT,
    created_at      TEXT NOT NULL DEFAULT (datetime('now'))
);
"""

STATUSES = {"pending", "paid", "shipped", "delivered", "cancelled"}


def open_shop() -> sqlite3.Connection:
    con = sqlite3.connect(SHOP_DB)
    con.row_factory = sqlite3.Row
    con.execute("PRAGMA foreign_keys = ON")
    return con


def seed_from_olist(force: bool = False) -> None:
    """Create the schema and populate it from the Olist dataset if empty."""
    con = open_shop()
    con.executescript(SCHEMA)

    cur = con.execute("SELECT COUNT(*) AS c FROM products")
    if cur.fetchone()["c"] > 0 and not force:
        con.close()
        return

    if force:
        for t in ("reviews", "payments", "order_items", "orders",
                  "cart_items", "carts", "products", "customers",
                  "sellers", "categories"):
            con.execute(f"DELETE FROM {t}")

    olist = sqlite3.connect(OLIST_DB)
    olist.row_factory = sqlite3.Row

    con.execute("BEGIN")

    # Categories (portuguese -> english translation table)
    cat_map: dict[str, int] = {}
    for r in olist.execute(
        "SELECT product_category_name, product_category_name_english "
        "FROM product_category_name_translation"
    ):
        name_pt = r["product_category_name"]
        slug = (name_pt or "unknown").replace(" ", "-").lower()
        cur = con.execute(
            "INSERT INTO categories (slug, name_pt, name_en) VALUES (?,?,?) "
            "ON CONFLICT(slug) DO UPDATE SET name_pt=excluded.name_pt",
            (slug, name_pt, r["product_category_name_english"]),
        )
        cat_map[name_pt] = cur.lastrowid
    # fallback category for NULL names
    cur = con.execute(
        "INSERT INTO categories (slug, name_pt, name_en) VALUES ('unknown','unknown','Unknown') "
        "ON CONFLICT(slug) DO UPDATE SET name_pt=excluded.name_pt"
    )
    cat_map[None] = cur.lastrowid

    # Sellers
    con.executemany(
        "INSERT INTO sellers (id, city, state) VALUES (?,?,?) "
        "ON CONFLICT(id) DO NOTHING",
        [
            (r["seller_id"], r["seller_city"], r["seller_state"])
            for r in olist.execute("SELECT seller_id, seller_city, seller_state FROM sellers")
        ],
    )

    # Customers
    con.executemany(
        "INSERT INTO customers (id, unique_id, city, state) VALUES (?,?,?,?) "
        "ON CONFLICT(id) DO NOTHING",
        [
            (r["customer_id"], r["customer_unique_id"],
             r["customer_city"], r["customer_state"])
            for r in olist.execute(
                "SELECT customer_id, customer_unique_id, customer_city, customer_state "
                "FROM customers"
            )
        ],
    )

    # Products: seed catalog with stock. Price = historical avg order_item price
    # for that product (one GROUP BY pass, not a per-row subquery). NULL -> a
    # deterministic placeholder. Stock is a deterministic 20..199 per product.
    avg_prices = {
        row["product_id"]: row["a"]
        for row in olist.execute(
            "SELECT product_id, AVG(price) AS a FROM order_items GROUP BY product_id"
        )
    }
    product_rows = []
    for r in olist.execute(
        "SELECT product_id, product_category_name, product_weight_g, "
        "product_length_cm, product_height_cm, product_width_cm FROM products"
    ):
        pid = r["product_id"]
        avg = avg_prices.get(pid)
        price = round(avg, 2) if avg else round((hash(pid) % 9000) / 100.0 + 9.9, 2)
        stock = 20 + (hash(pid) % 180)  # 20..199 deterministic
        product_rows.append((
            pid,
            cat_map.get(r["product_category_name"], cat_map[None]),
            r["product_category_name"],
            r["product_weight_g"],
            r["product_length_cm"],
            r["product_height_cm"],
            r["product_width_cm"],
            price,
            stock,
        ))
    con.executemany(
        "INSERT INTO products (id, category_id, name_pt, weight_g, length_cm, "
        "height_cm, width_cm, price, stock) VALUES (?,?,?,?,?,?,?,?,?) "
        "ON CONFLICT(id) DO NOTHING",
        product_rows,
    )

    con.commit()
    print(
        f"seeded shop.db: {len(cat_map)} categories, "
        f"{con.execute('SELECT COUNT(*) c FROM sellers').fetchone()['c']} sellers, "
        f"{con.execute('SELECT COUNT(*) c FROM customers').fetchone()['c']} customers, "
        f"{con.execute('SELECT COUNT(*) c FROM products').fetchone()['c']} products"
    )
    con.close()
    olist.close()


if __name__ == "__main__":
    t0 = time.time()
    seed_from_olist(force="--force" in __import__("sys").argv)
    print(f"done in {time.time()-t0:.2f}s")
