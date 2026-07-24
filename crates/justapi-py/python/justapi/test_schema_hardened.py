"""Unit tests for the hardened `justapi.Schema` validator (P3 gap #3).

Covers: Field constraints (min/max length, numeric bounds, regex,
enum, format), nested Schema models via $ref/$defs, and array-of-model.
The validation itself runs in Rust (jsonschema) with zero Python
round-trips on the hot path.

Also covers the regression for gap #3's server-path bug: request-body
validation is performed by `justapi_core::validate::CompiledValidator`,
which must register the built-in `format` checkers — otherwise `format`
violations were silently accepted on the live HTTP path (only the
in-process `validate_value` path enforced them).
"""
import json
import threading
import time
import urllib.error
import urllib.request

from justapi import JustAPIApp, Schema, Field
from justapi._justapi import validate_value


class Address(Schema):
    street: str = Field(min_length=1, max_length=200)
    zip: str = Field(regex=r"^\d{5}$")


class Order(Schema):
    id: int = Field(ge=1)
    total: float = Field(ge=0.0, le=1_000_000)


class User(Schema):
    name: str = Field(min_length=1, max_length=50, regex=r"^[a-zA-Z ]+$")
    email: str = Field(format="email")
    age: int | None = Field(default=None, gt=0, le=120)
    role: str = Field(enum=["admin", "user"])
    address: Address
    orders: list[Order]


def _schema():
    return User._schema_json()


def _check(value):
    return validate_value(_schema(), json.dumps(value))


def test_valid_nested_payload_passes():
    good = {
        "name": "Bob Smith",
        "email": "bob@example.com",
        "age": 30,
        "role": "admin",
        "address": {"street": "1 Main", "zip": "12345"},
        "orders": [{"id": 1, "total": 9.99}],
    }
    assert _check(good) == [], "expected valid payload to pass"


def test_nested_bad_values_reported():
    bad = {
        "name": "x",
        "email": "nope",
        "age": 200,
        "role": "root",
        "address": {"street": "", "zip": "abc"},
        "orders": [{"id": 0, "total": -1}],
    }
    errs = _check(bad)
    # name too short, email bad, age>max, role bad, street empty,
    # zip bad, order id<min, order total<min => >=7 distinct errors.
    assert len(errs) >= 7, f"expected many errors, got {errs}"
    joined = "\n".join(errs).lower()
    assert "email" in joined
    assert "maximum" in joined or "120" in joined


def test_schema_emits_defs_and_refs():
    s = json.loads(_schema())
    # Nested models are lifted into $defs and referenced via $ref.
    assert "$defs" in s
    assert "Address" in s["$defs"]
    assert "Order" in s["$defs"]
    assert s["properties"]["address"] == {"$ref": "#/$defs/Address"}
    assert s["properties"]["orders"]["type"] == "array"
    assert s["properties"]["orders"]["items"] == {"$ref": "#/$defs/Order"}


def test_format_keyword_enforced():
    # email format is asserted by the Rust-side format registry.
    assert validate_value('{"type":"string","format":"email"}', '"nope"') != []
    assert validate_value('{"type":"string","format":"email"}', '"a@b.com"') == []
    # uuid / date-time / ipv4 also enforced.
    assert validate_value('{"type":"string","format":"uuid"}', '"bad"') != []
    assert (
        validate_value(
            '{"type":"string","format":"uuid"}',
            '"123e4567-e89b-12d3-a456-426614174000"',
        )
        == []
    )


def test_optional_field_not_required():
    s = json.loads(_schema())
    # age is Optional (default None) => not in required.
    assert "age" not in s["required"]
    # A payload without age / orders / address should still validate
    # (age optional, but address + orders are required here).
    partial = {
        "name": "Solo",
        "email": "solo@example.com",
        "role": "user",
        "address": {"street": "9 Ave", "zip": "99999"},
        "orders": [],
    }
    assert _check(partial) == []


def _post(addr, payload):
    req = urllib.request.Request(
        f"http://{addr}/users",
        data=json.dumps(payload).encode(),
        headers={"Content-Type": "application/json"},
        method="POST",
    )
    try:
        return urllib.request.urlopen(req).status
    except urllib.error.HTTPError as e:
        return e.code


def test_server_path_enforces_format():
    """Regression: `format` must be asserted on the live HTTP path.

    Previously the server-side `CompiledValidator` did not register the
    `format` checkers, so an `email: "nope"` payload was accepted (200)
    even though in-process `validate_value` rejected it (422).
    """
    app = JustAPIApp()

    async def create_user(request):
        return {"status": 201, "body": request["body"], "headers": []}

    app.post("/users", create_user, schema=User)

    addr = "127.0.0.1:9871"
    t = threading.Thread(target=lambda: app.run(addr), daemon=True)
    t.start()
    time.sleep(1.0)

    good = {
        "name": "Bob Smith",
        "email": "bob@example.com",
        "age": 30,
        "role": "admin",
        "address": {"street": "1 Main", "zip": "12345"},
        "orders": [{"id": 1, "total": 9.99}],
    }
    bad_email = dict(good, email="nope")
    bad_zip = dict(good, address={"street": "1 Main", "zip": "abc"})

    assert _post(addr, good) == 201, "valid payload should be accepted"
    assert _post(addr, bad_email) == 422, "format violation must be rejected"
    assert _post(addr, bad_zip) == 422, "nested constraint must be rejected"

