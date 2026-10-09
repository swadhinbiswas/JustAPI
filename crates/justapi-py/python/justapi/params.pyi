from _typeshed import Incomplete
from typing import Any

class Param:
    default: Incomplete
    alias: Incomplete
    gt: Incomplete
    ge: Incomplete
    lt: Incomplete
    le: Incomplete
    multiple_of: Incomplete
    min_length: Incomplete
    max_length: Incomplete
    pattern: Incomplete
    embed: Incomplete
    title: Incomplete
    description: Incomplete
    deprecated: Incomplete
    include_in_schema: Incomplete
    def __init__(self, default: Any = ..., alias: str | None = None, gt: Any = ..., ge: Any = ..., lt: Any = ..., le: Any = ..., multiple_of: Any = ..., min_length: int | None = ..., max_length: int | None = ..., pattern: str | None = ..., embed: bool = ..., title: str | None = ..., description: str | None = ..., deprecated: bool = ..., include_in_schema: bool = ...) -> None: ...

# Runtime these are classes; declared as functions returning Any so the
# FastAPI-style default idiom type-checks: `q: int = Query(1, ge=0)`.
def Path(default: Any = ..., *, alias: str | None = ..., gt: Any = ..., ge: Any = ..., lt: Any = ..., le: Any = ..., multiple_of: Any = ..., title: str | None = ..., description: str | None = ..., deprecated: bool = ..., include_in_schema: bool = ...) -> Any: ...
def Query(default: Any = ..., *, alias: str | None = ..., gt: Any = ..., ge: Any = ..., lt: Any = ..., le: Any = ..., multiple_of: Any = ..., min_length: int | None = ..., max_length: int | None = ..., pattern: str | None = ..., embed: bool = ..., title: str | None = ..., description: str | None = ..., deprecated: bool = ..., include_in_schema: bool = ...) -> Any: ...
def Header(default: Any = ..., *, alias: str | None = ..., convert_underscores: bool = ..., gt: Any = ..., ge: Any = ..., lt: Any = ..., le: Any = ..., multiple_of: Any = ..., min_length: int | None = ..., max_length: int | None = ..., pattern: str | None = ..., embed: bool = ..., title: str | None = ..., description: str | None = ..., deprecated: bool = ..., include_in_schema: bool = ...) -> Any: ...
def Cookie(default: Any = ..., *, alias: str | None = ..., title: str | None = ..., description: str | None = ..., deprecated: bool = ..., include_in_schema: bool = ...) -> Any: ...
def Body(default: Any = ..., *, alias: str | None = ...) -> Any: ...
def File(default: Any = ..., *, alias: str | None = ...) -> Any: ...
def Form(default: Any = ..., *, alias: str | None = ...) -> Any: ...

def coerce_param_value(val: Any, annotation: Any, alias: str, spec: Any = ...) -> Any: ...
