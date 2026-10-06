import inspect
from typing import Any, Optional

class Param:
    def __init__(self, default: Any = ..., alias: Optional[str] = None):
        self.default = default
        self.alias = alias

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


def coerce_param_value(val: Any, annotation: Any, alias: str) -> Any:
    """Coerce a raw str param to its annotation. Raises ValueError on failure."""
    if val is None or annotation is inspect.Parameter.empty:
        return val
    if isinstance(val, str):
        if annotation is bool:
            low = val.strip().lower()
            if low in _TRUE_SET:
                return True
            if low in _FALSE_SET:
                return False
            raise ValueError(f"invalid boolean value {val!r}")
        if annotation is int and not isinstance(val, bool):
            return int(val.strip())
        if annotation is float:
            return float(val.strip())
    return val
class Cookie(Param):
    pass

class Body(Param):
    pass

class File(Param):
    pass

class Form(Param):
    pass
