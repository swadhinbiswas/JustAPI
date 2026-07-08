from typing import Any, Optional

class Param:
    def __init__(self, default: Any = ..., alias: Optional[str] = None):
        self.default = default
        self.alias = alias

class Path(Param): pass
class Query(Param): pass
class Header(Param): pass
class Cookie(Param):
    pass

class Body(Param):
    pass

class File(Param):
    pass

class Form(Param):
    pass
