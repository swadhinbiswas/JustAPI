"""shop — a proper MVP e-commerce store API built on the JustAPI Runtime.

A real mini shop: categories, products (managed stock & price), sellers,
customers, a cart, checkout that creates orders + payments and decrements
inventory, and customer reviews. Data lives in `shop.db` (created/seeded by
`db_setup.py` from the Olist dataset). Mutations use real SQL transactions.

Package layout
--------------
  shop/config.py     application instance + DB wiring (singletons)
  shop/db.py         SQL helpers (query / paginate / tokens)
  shop/schemas.py    request validation schemas + validate() helper
  shop/catalog.py    categories + products CRUD + sellers
  shop/customers.py  customer register/lookup
  shop/cart.py       cart lifecycle + checkout (order + payment + stock dec)
  shop/orders.py     orders list/detail/status lifecycle
  shop/reviews.py    reviews + analytics

Importing this package builds the `app` (connects the DB and registers all
routes). Run with:  python -m shop --port 8080
"""
from .config import app, db

__all__ = ["app", "db"]
