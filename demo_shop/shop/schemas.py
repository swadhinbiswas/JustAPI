"""shop.schemas — request validation schemas and a validation helper.

Schemas drive OpenAPI docs via `body_schema=`. Because the framework currently
delivers the validated `body_schema` payload to handlers as raw `bytes` (see
ADR-069), handlers parse `request.json()` and validate through `validate()`,
which uses the Schema's generated JSON Schema.
"""
from __future__ import annotations

from typing import Any, Optional

from justapi import HTTPException, Schema


def slugify(value: str) -> str:
    """ASCII slug: lowercase, spaces/punctuation -> '-', collapse repeats."""
    out = []
    prev_dash = False
    for ch in value.lower():
        if ch.isalnum():
            out.append(ch)
            prev_dash = False
        elif not prev_dash:
            out.append("-")
            prev_dash = True
    s = "".join(out).strip("-")
    while "--" in s:
        s = s.replace("--", "-")
    return s or "category"


class CategoryIn(Schema):
    slug: Optional[str] = None
    name_pt: str
    name_en: Optional[str] = None


class ProductIn(Schema):
    id: str
    category_id: Optional[int] = None
    name_pt: Optional[str] = None
    weight_g: Optional[float] = None
    length_cm: Optional[float] = None
    height_cm: Optional[float] = None
    width_cm: Optional[float] = None
    price: float
    stock: int = 0


class ProductPatch(Schema):
    price: Optional[float] = None
    stock: Optional[int] = None
    active: Optional[int] = None


class CustomerIn(Schema):
    id: str
    unique_id: Optional[str] = None
    city: Optional[str] = None
    state: Optional[str] = None
    email: Optional[str] = None


class CartItemIn(Schema):
    product_id: str
    quantity: int = 1


class CheckoutIn(Schema):
    method: str = "credit_card"
    installments: int = 1


class OrderStatusIn(Schema):
    status: str  # pending|paid|shipped|delivered|cancelled


class ReviewIn(Schema):
    product_id: str
    score: int
    title: Optional[str] = None
    comment: Optional[str] = None


class _NS:
    """Namespace view over a validated dict (attribute + item access)."""

    def __init__(self, data: dict):
        self.__dict__.update(data)

    def __getattr__(self, k):
        # Optional fields that were absent from the payload default to None.
        return None

    def __getitem__(self, k):
        return self.__dict__[k]


def validate(schema_cls, data) -> _NS:
    """Validate `data` (already-parsed dict) against a justapi.Schema subclass.

    Returns a namespace object with coerced fields. Raises HTTPException(422)
    with a descriptive detail on the first validation failure.
    """
    if not isinstance(data, dict):
        raise HTTPException(status_code=422, detail="body must be a JSON object")
    schema = schema_cls._build_schema()
    required = set(schema.get("required", []))
    props = schema.get("properties", {})
    out = {}
    for name, prop in props.items():
        present = name in data and data[name] is not None
        if not present:
            if name in required:
                raise HTTPException(status_code=422, detail=f"missing required field: {name}")
            # Use the Schema class attribute as the field's default (e.g. 1 for
            # installments); fall back to None only if no default is declared.
            default = getattr(schema_cls, name, None)
            out[name] = default if not callable(default) else None
            continue
        val = data[name]
        jt = prop.get("type")
        try:
            if jt == "integer":
                out[name] = int(val)
            elif jt == "number":
                out[name] = float(val)
            elif jt == "boolean":
                out[name] = bool(val)
            else:
                out[name] = val
        except (ValueError, TypeError):
            raise HTTPException(
                status_code=422, detail=f"field {name} must be {jt}, got {type(val).__name__}"
            )
    return _NS(out)
