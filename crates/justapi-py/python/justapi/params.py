import inspect
from typing import Any, Optional

class Param:
    def __init__(
        self,
        default: Any = ...,
        alias: Optional[str] = None,
        gt: Any = None,
        ge: Any = None,
        lt: Any = None,
        le: Any = None,
        min_length: Optional[int] = None,
        max_length: Optional[int] = None,
        pattern: Optional[str] = None,
    ):
        self.default = default
        self.alias = alias
        self.gt = gt
        self.ge = ge
        self.lt = lt
        self.le = le
        self.min_length = min_length
        self.max_length = max_length
        self.pattern = pattern

class Path(Param): pass
class Query(Param): pass
class Header(Param):
    def __init__(
        self,
        default: Any = ...,
        alias: Optional[str] = None,
        convert_underscores: bool = True,
    ):
        super().__init__(default, alias)
        self.convert_underscores = convert_underscores


_TRUE_SET = {"1", "true", "yes", "on"}
_FALSE_SET = {"0", "false", "no", "off", ""}


def coerce_param_value(val: Any, annotation: Any, alias: str, spec: Any = None) -> Any:
    """Coerce a raw str param to its annotation, then enforce Param constraints.

    Raises ValueError on failure (caller maps to 422).
    """
    out = val
    if out is not None and annotation is not inspect.Parameter.empty and isinstance(out, str):
        if annotation is bool:
            low = out.strip().lower()
            if low in _TRUE_SET:
                out = True
            elif low in _FALSE_SET:
                out = False
            else:
                raise ValueError(f"invalid boolean value {val!r}")
        elif annotation is int and not isinstance(out, bool):
            out = int(out.strip())
        elif annotation is float:
            out = float(out.strip())
    if spec is not None and out is not None:
        _check_constraints(out, spec, alias)
    return out


def _check_constraints(val: Any, spec: Any, alias: str) -> None:
    gt = getattr(spec, "gt", None)
    ge = getattr(spec, "ge", None)
    lt = getattr(spec, "lt", None)
    le = getattr(spec, "le", None)
    min_length = getattr(spec, "min_length", None)
    max_length = getattr(spec, "max_length", None)
    pattern = getattr(spec, "pattern", None)
    if gt is not None and not val > gt:
        raise ValueError(f"{alias} must be > {gt}")
    if ge is not None and not val >= ge:
        raise ValueError(f"{alias} must be >= {ge}")
    if lt is not None and not val < lt:
        raise ValueError(f"{alias} must be < {lt}")
    if le is not None and not val <= le:
        raise ValueError(f"{alias} must be <= {le}")
    if min_length is not None and len(val) < min_length:
        raise ValueError(f"{alias} length must be >= {min_length}")
    if max_length is not None and len(val) > max_length:
        raise ValueError(f"{alias} length must be <= {max_length}")
    if pattern is not None:
        import re
        if not re.search(pattern, str(val)):
            raise ValueError(f"{alias} must match pattern {pattern!r}")
class Cookie(Param):
    pass

class Body(Param):
    pass

class File(Param):
    pass

class Form(Param):
    pass
