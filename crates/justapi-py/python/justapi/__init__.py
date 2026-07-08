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

from . import testing as testing
from . import tracing as tracing

from ._justapi import serve, TokenStreamResponse, WebSocket, Dag, DagNode, RateLimiter, RateLimitResult, Request, UploadFile  # type: ignore[import-untyped]
from ._justapi import Database  # type: ignore[import-untyped]
from .app import JustAPIApp, Depends, adaptive_batch, APIRouter, Controller, controller, route_get, route_post, route_put, route_patch, route_delete, route_query, route_sse, route_websocket, JustAPITestClient, RequestValidationError
from .templating import Jinja2Templates
from .background import BackgroundTasks
from .responses import Response, HTMLResponse, PlainTextResponse, JSONResponse, RedirectResponse, StreamingResponse
from .params import Param, Path, Query, Header, Cookie, Body, File, Form

# ---------------------------------------------------------------------------
# Fast Schema — Rust-native JSON Schema validation
# ---------------------------------------------------------------------------

import json
import typing
import dataclasses
import sys
import types as _types


class Schema:
    """Base class for defining Rust-native validation schemas.

    Subclass this and add type-annotated fields:

        class UserSchema(Schema):
            name: str
            email: str
            age: int | None = None

    The resulting JSON Schema is compiled in Rust and validated
    with zero Python round-trips.
    """

    _field_annotations: dict[str, tuple[type, typing.Any]]

    def __init_subclass__(cls, **kwargs):
        super().__init_subclass__(**kwargs)
        annotations = getattr(cls, '__annotations__', {})
        fields = {}
        for field_name, field_type in annotations.items():
            if field_name.startswith('_'):
                continue
            default = cls.__dict__.get(field_name, dataclasses.MISSING)
            fields[field_name] = (field_type, default)
        cls._field_annotations = fields

    @classmethod
    def _schema_json(cls) -> str:
        """Generate a JSON Schema string from the class field definitions."""
        return json.dumps(cls._build_schema())

    @classmethod
    def _build_schema(cls) -> dict:
        """Build a JSON Schema dict from field annotations."""
        properties = {}
        required = []
        for field_name, (field_type, default) in cls._field_annotations.items():
            prop = _type_to_json_schema(field_type)
            properties[field_name] = prop
            if default is dataclasses.MISSING:
                required.append(field_name)
        return {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": False,
        }


def _type_to_json_schema(tp: type) -> dict:
    """Convert a Python type annotation to a JSON Schema property."""
    origin = typing.get_origin(tp)
    args = typing.get_args(tp)

    # Optional[X] is Union[X, None]
    if origin is typing.Union or origin is _types.UnionType:
        non_none = [a for a in args if a is not type(None)]
        if len(non_none) == 1:
            return _type_to_json_schema(non_none[0])

    if tp is str:
        return {"type": "string"}
    if tp is int:
        return {"type": "integer"}
    if tp is float:
        return {"type": "number"}
    if tp is bool:
        return {"type": "boolean"}
    if tp is bytes:
        return {"type": "string", "contentEncoding": "base64"}
    if origin is list:
        item_type = args[0] if args else str
        return {"type": "array", "items": _type_to_json_schema(item_type)}
    if origin is dict:
        value_type = args[1] if len(args) > 1 else str
        return {"type": "object", "additionalProperties": _type_to_json_schema(value_type)}
    return {"type": "string"}


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
    "serve", "JustAPIApp", "Depends", "Database", "Schema", "pydantic_schema", 
    "JustAPITestClient", "testing", "tracing", "Jinja2Templates", "BackgroundTasks", 
    "TokenStreamResponse", "WebSocket", "Dag", "DagNode", "RateLimiter", "RateLimitResult",
    "adaptive_batch", "APIRouter", "Controller", "controller", 
    "route_get", "route_post", "route_put", "route_patch", "route_delete", "route_query", 
    "route_sse", "route_websocket", "Request", "Response", "HTMLResponse", 
    "PlainTextResponse", "JSONResponse", "RedirectResponse", "StreamingResponse",
    "UploadFile", "RequestValidationError", "Param", "Path", "Query", "Header", 
    "Cookie", "Body", "File", "Form"
]
