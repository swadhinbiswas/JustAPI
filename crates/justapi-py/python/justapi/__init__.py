"""JustAPI Runtime — Python bindings.

Usage:
    import justapi
    justapi.serve("127.0.0.1:8080")

    # Tier B Native API (FastAPI replacement)
    from justapi import JustAPIApp, Schema

    class UserSchema(Schema):
        name: str
        email: str
        age: int | None = None

    app = JustAPIApp()
    app.get("/hello", lambda r: {"message": "hello"})
    app.post("/users", handler, schema=UserSchema)
    app.run("127.0.0.1:8080")

    # Pydantic models are also supported:
    from justapi import pydantic_schema
    from pydantic import BaseModel

    app.post("/users", handler, schema=pydantic_schema(UserModel))

"""

__version__ = "2.0.10"

# The compiled core MUST be imported first: submodules (auth, testing, ...)
# do `from ._justapi import ...`, and on free-threaded builds a submodule
# import that precedes the core import can resolve `_justapi` differently.
from ._justapi import serve, TokenStreamResponse, ValidatedStreamResponse, Dag, DagNode, Request, HTTPConnection, Headers, QueryParams, URL, State, RequestStream, UploadFile  # type: ignore[import-untyped]

from . import auth as auth
from .auth import (  # noqa: F401 — top-level re-export (FastAPI parity, used by docs)
    JwtAuth,
    OAuth2PasswordBearer,
    OAuth2PasswordRequestForm,
    OAuth2PasswordRequestFormStrict,
)
from . import testing as testing
from . import tracing as tracing
from . import status as status

# RateLimiter is behind the redis-rate-limit feature flag
try:
    from ._justapi import RateLimiter, RateLimitResult  # type: ignore[import-untyped]
except ImportError:
    RateLimiter = None  # type: ignore[assignment,misc]
    RateLimitResult = None  # type: ignore[assignment,misc]
from ._justapi import Database, DbPool, DbParam  # type: ignore[import-untyped]
from .app import JustAPIApp, JustAPI, JustAPP, Depends, Security, Mailer, adaptive_batch, native_async, APIRouter, Controller, controller, route_get, route_post, route_put, route_patch, route_delete, route_query, route_sse, route_websocket, JustAPITestClient, RequestValidationError, Session
from .system import build_help, build_openapi, register_system_routes
from .exceptions import HTTPException, WebSocketException
from .websockets import WebSocket, WebSocketState, WebSocketDisconnect
from .templating import Jinja2Templates
from .background import BackgroundTasks
from ._justapi import Scheduler  # type: ignore[import-untyped]

# Logging / tracing configuration (thin re-exports of justapi-core's
# `tracing`-based subsystem). `app.run()` installs a default INFO text logger
# automatically; these let apps opt into JSON / file / OTLP instead.
from ._justapi import (  # type: ignore[import-untyped]
    init_logging,
    init_json_logging,
    init_file_logging,
    init_otlp_tracing,
    shutdown_tracing,
)
from . import logging as logging  # re-export the logging namespace
from .responses import (
    Response,
    HTMLResponse,
    PlainTextResponse,
    JSONResponse,
    RedirectResponse,
    StreamingResponse,
    FileResponse,
)
from .params import Param, Path, Query, Header, Cookie, Body, File, Form

# ---------------------------------------------------------------------------
# Fast Schema — Rust-native JSON Schema validation
# ---------------------------------------------------------------------------

import json
import typing
import dataclasses
import sys
import types as _types


class Field:
    """Per-field metadata for a :class:`Schema` field.

    Carries validation constraints that are emitted as JSON Schema keywords
    and enforced by the Rust-native ``jsonschema`` validator (no Python
    round-trip on the hot path). Mirrors the common Pydantic ``Field(...)``
    surface so existing models port cheaply.

        class User(Schema):
            name: str = Field(min_length=1, max_length=50, regex=r"^[a-z]+$")
            age: int | None = Field(default=None, gt=0, le=120)
            email: str = Field(format="email")
            role: str = Field(enum=["admin", "user"])
            address: Address          # nested Schema (emits a $ref)
            orders: list[Order]       # array of nested Schema

    Numeric bounds: ``gt``/``ge``/``lt``/``le`` (or ``exclusive_minimum`` /
    ``minimum`` etc.). String bounds: ``min_length``/``max_length`` /
    ``regex`` (``pattern``) / ``format`` (``email``, ``date-time``, ...).
    ``enum`` restricts to a fixed set of values. ``default`` sets the value
    used when the field is omitted (and removes it from ``required``).
    """

    __slots__ = (
        "default", "gt", "ge", "lt", "le",
        "min_length", "max_length", "regex", "format",
        "enum", "description",
    )

    def __init__(
        self,
        *,
        default=dataclasses.MISSING,
        gt=None, ge=None, lt=None, le=None,
        min_length=None, max_length=None,
        regex=None, format=None,
        enum=None, description=None,
    ):
        self.default = default
        self.gt = gt
        self.ge = ge
        self.lt = lt
        self.le = le
        self.min_length = min_length
        self.max_length = max_length
        self.regex = regex
        self.format = format
        self.enum = enum
        self.description = description

    def apply(self, prop: dict) -> dict:
        """Layer this field's constraints onto a base JSON Schema property."""
        if self.gt is not None:
            prop["exclusiveMinimum"] = self.gt
        if self.ge is not None:
            prop["minimum"] = self.ge
        if self.lt is not None:
            prop["exclusiveMaximum"] = self.lt
        if self.le is not None:
            prop["maximum"] = self.le
        if self.min_length is not None:
            prop["minLength"] = self.min_length
        if self.max_length is not None:
            prop["maxLength"] = self.max_length
        if self.regex is not None:
            prop["pattern"] = self.regex
        if self.format is not None:
            prop["format"] = self.format
        if self.enum is not None:
            prop["enum"] = list(self.enum)
        if self.description is not None:
            prop["description"] = self.description
        return prop


class Schema:
    """Base class for defining Rust-native validation schemas.

    Subclass this and add type-annotated fields. Constraints are expressed
    with :class:`Field`; nested models are expressed by typing a field as
    another ``Schema`` subclass (or ``list[OtherSchema]``). The resulting
    JSON Schema (with ``$ref`` / ``$defs`` for nesting) is compiled in
    Rust and validated with zero Python round-trips.

        class UserSchema(Schema):
            name: str = Field(min_length=1, max_length=50)
            email: str = Field(format="email")
            age: int | None = None
            address: Address
            orders: list[Order]
    """

    _field_annotations: dict[str, tuple[type, typing.Any]]
    _defs: dict[str, dict]

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        # Resolve annotations via get_type_hints so that `from __future__ import
        # annotations` (which stringizes them) and forward references still map
        # to real types — otherwise every field would be treated as `str`.
        try:
            annotations = typing.get_type_hints(cls)
        except Exception:
            annotations = getattr(cls, '__annotations__', {})
        fields = {}
        for field_name, field_type in annotations.items():
            if field_name.startswith('_'):
                continue
            raw = cls.__dict__.get(field_name, dataclasses.MISSING)
            # A Field(...) carries its own default; a bare value is the default.
            if isinstance(raw, Field):
                default = raw.default
            else:
                default = raw
            fields[field_name] = (field_type, default)
        cls._field_annotations = fields
        cls._defs = {}

    @classmethod
    def _schema_json(cls) -> str:
        """Generate a JSON Schema string from the class field definitions."""
        return json.dumps(cls._build_schema())

    @classmethod
    def _build_schema(cls) -> dict:
        """Build a JSON Schema dict, collecting nested ``$defs``."""
        defs: dict[str, dict] = {}
        schema = cls._build_schema_with_defs(defs)
        if defs:
            schema["$defs"] = defs
        return schema

    @classmethod
    def _build_schema_with_defs(cls, defs: dict) -> dict:
        """Build this model's schema, lifting any nested models into ``defs``."""
        properties = {}
        required = []
        for field_name, (field_type, default) in cls._field_annotations.items():
            prop, _nested = _type_to_json_schema(field_type, defs)
            # Apply per-field constraints from a Field(...) descriptor.
            raw = cls.__dict__.get(field_name)
            if isinstance(raw, Field):
                prop = raw.apply(prop)
            properties[field_name] = prop
            if default is dataclasses.MISSING:
                required.append(field_name)
        return {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": False,
        }


def _type_to_json_schema(tp: type, defs: dict) -> tuple[dict, bool]:
    """Convert a Python type annotation to a JSON Schema property.

    ``defs`` accumulates nested ``Schema`` definitions (keyed by class name)
    so callers can lift them into a top-level ``$defs``. Returns
    ``(property, unused)`` — the bool is reserved and currently always False.
    """
    origin = typing.get_origin(tp)
    args = typing.get_args(tp)

    # Optional[X] is Union[X, None]
    if origin is typing.Union or origin is _types.UnionType:
        non_none = [a for a in args if a is not type(None)]
        if len(non_none) == 1:
            return _type_to_json_schema(non_none[0], defs)

    # A nested Schema subclass becomes a $ref into $defs.
    if isinstance(tp, type) and issubclass(tp, Schema):
        name = tp.__name__
        if name not in defs:
            defs[name] = tp._build_schema_with_defs(defs)
        return ({"$ref": f"#/$defs/{name}"}, False)

    if tp is str:
        return ({"type": "string"}, False)
    if tp is int:
        return ({"type": "integer"}, False)
    if tp is float:
        return ({"type": "number"}, False)
    if tp is bool:
        return ({"type": "boolean"}, False)
    if tp is bytes:
        return ({"type": "string", "contentEncoding": "base64"}, False)
    if origin is list:
        item_type = args[0] if args else str
        return ({"type": "array", "items": _type_to_json_schema(item_type, defs)[0]}, False)
    if origin is dict:
        value_type = args[1] if len(args) > 1 else str
        return ({"type": "object", "additionalProperties": _type_to_json_schema(value_type, defs)[0]}, False)
    return ({"type": "string"}, False)


def pydantic_schema(model_class) -> str:
    """Extract a JSON Schema string from a Pydantic BaseModel subclass.

    Usage:
        from justapi import pydantic_schema
        from pydantic import BaseModel

        class UserModel(BaseModel):
            name: str
            email: str

    app.post("/users", handler, schema=pydantic_schema(UserModel))
    """
    if hasattr(model_class, 'model_json_schema'):
        # Pydantic v2
        schema = model_class.model_json_schema()
    elif hasattr(model_class, 'schema_json'):
        # Pydantic v1
        schema = model_class.schema()
    else:
        raise TypeError(
            f"{model_class} does not appear to be a Pydantic BaseModel "
            "(no model_json_schema or schema method)"
        )
    return json.dumps(schema)


__all__ = [
    "__version__", "serve", "JustAPIApp", "JustAPI", "JustAPP", "Depends", "Security", "Mailer", "Database", "DbPool", "DbParam", "Schema", "pydantic_schema", 
    "JustAPITestClient", "testing", "tracing", "auth", "Jinja2Templates", "BackgroundTasks", "Scheduler", 
    "TokenStreamResponse", "ValidatedStreamResponse", "WebSocket", "Dag", "DagNode", "RateLimiter", "RateLimitResult",
    "adaptive_batch", "native_async", "APIRouter", "Controller", "controller", 
    "route_get", "route_post", "route_put", "route_patch", "route_delete", "route_query", 
    "route_sse", "route_websocket", "Request", "HTTPConnection", "Headers",
    "QueryParams", "URL", "State", "Session", "RequestStream", "Response", "HTMLResponse", 
    "PlainTextResponse", "JSONResponse", "RedirectResponse", "StreamingResponse",
    "FileResponse",
    "UploadFile", "RequestValidationError", "Param", "Path", "Query", "Header", 
    "Cookie", "Body", "File", "Form", "status", "HTTPException", "WebSocketException",
    "WebSocketState", "WebSocketDisconnect", "JwtAuth", "OAuth2PasswordBearer",
    "OAuth2PasswordRequestForm", "OAuth2PasswordRequestFormStrict",
    "build_help", "build_openapi", "register_system_routes"
]
