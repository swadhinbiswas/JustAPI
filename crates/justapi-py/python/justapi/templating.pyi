import jinja2
from _typeshed import Incomplete
from typing import Any

class Jinja2Templates:
    directory: Incomplete
    env: Incomplete
    def __init__(self, directory: str) -> None: ...
    def get_template(self, name: str) -> jinja2.Template: ...
    def TemplateResponse(self, name: str, context: dict[str, Any], status_code: int = 200, headers: list = None) -> dict: ...
