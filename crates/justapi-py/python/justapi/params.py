import enum
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
        multiple_of: Any = None,
        min_length: Optional[int] = None,
        max_length: Optional[int] = None,
        pattern: Optional[str] = None,
        title: Optional[str] = None,
        description: Optional[str] = None,
        deprecated: bool = False,
        include_in_schema: bool = True,
    ):
        self.default = default
        self.alias = alias
        self.gt = gt
        self.ge = ge
        self.lt = lt
        self.le = le
        self.multiple_of = multiple_of
        self.min_length = min_length
        self.max_length = max_length
        self.pattern = pattern
        self.title = title
        self.description = description
        self.deprecated = deprecated
        self.include_in_schema = include_in_schema

class Path(Param): pass
class Query(Param): pass
class Header(Param):
    def __init__(
        self,
        default: Any = ...,
        alias: Optional[str] = None,
        convert_underscores: bool = True,
        **kwargs: Any,
    ):
        super().__init__(default, alias, **kwargs)
        self.convert_underscores = convert_underscores


_TRUE_SET = {"1", "true", "yes", "on"}
_FALSE_SET = {"0", "false", "no", "off", ""}


def is_list_annotation(annotation: Any) -> bool:
    """True for ``list``, ``List``, ``list[X]`` / ``List[X]`` annotations."""
    if annotation is list:
        return True
    import typing
    return typing.get_origin(annotation) in (list,)


def list_item_type(annotation: Any) -> Any:
    import typing
    args = typing.get_args(annotation)
    return args[0] if args else inspect.Parameter.empty


def coerce_param_value(val: Any, annotation: Any, alias: str, spec: Any = None) -> Any:
    """Coerce a raw str param to its annotation, then enforce Param constraints.

    Raises ValueError on failure (caller maps to 422).
    """
    out = val
    if isinstance(out, (list, tuple)) and is_list_annotation(annotation):
        item_ann = list_item_type(annotation)
        out = [coerce_param_value(v, item_ann, alias) for v in out]
    elif out is not None and annotation is not inspect.Parameter.empty and isinstance(out, str):
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
    if inspect.isclass(annotation) and issubclass(annotation, enum.Enum):
        if not isinstance(out, annotation):
            try:
                out = annotation(out)
            except ValueError:
                try:
                    out = annotation(int(str(out).strip()))
                except (ValueError, TypeError):
                    raise ValueError(f"invalid enum value {val!r}")
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
    if isinstance(val, (int, float)) and not isinstance(val, bool):
        if gt is not None and not val > gt:
            raise ValueError(f"{alias} must be > {gt}")
        if ge is not None and not val >= ge:
            raise ValueError(f"{alias} must be >= {ge}")
        if lt is not None and not val < lt:
            raise ValueError(f"{alias} must be < {lt}")
        if le is not None and not val <= le:
            raise ValueError(f"{alias} must be <= {le}")
        multiple_of = getattr(spec, "multiple_of", None)
        if multiple_of is not None:
            remainder = abs(val % multiple_of)
            if not (remainder < 1e-9 or abs(remainder - multiple_of) < 1e-9):
                raise ValueError(f"{alias} must be a multiple of {multiple_of}")
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
