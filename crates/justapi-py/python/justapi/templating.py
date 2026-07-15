import os
from typing import Any, Dict

try:
    import jinja2
except ImportError:
    jinja2 = None

class Jinja2Templates:
    """
    Jinja2 templating integration for JustAPI.
    
    Usage:
        templates = Jinja2Templates(directory="templates")
        
        @app.get("/html")
        def render_html(request):
            return templates.TemplateResponse("index.html", {"request": request, "name": "World"})
    """
    
    def __init__(self, directory: str):
        if jinja2 is None:
            raise RuntimeError(
                "jinja2 is required to use Jinja2Templates. "
                "Install it with: pip install jinja2"
            )
            
        self.directory = directory
        self.env = jinja2.Environment(
            loader=jinja2.FileSystemLoader(directory),
            autoescape=True
        )

    def get_template(self, name: str) -> "jinja2.Template":
        return self.env.get_template(name)

    def TemplateResponse(self, name: str, context: Dict[str, Any], status_code: int = 200, headers: list = None) -> dict:
        """
        Render a template and return a dictionary that JustAPI can automatically convert to a response.
        """
        template = self.get_template(name)
        # Mirror Starlette/FastAPI: expose `url_for` inside templates when a
        # `request` with a `url_for` method is present (e.g. a named route built
        # via `app.get(..., name="...")`).
        if "request" in context and hasattr(context["request"], "url_for"):
            context.setdefault("url_for", context["request"].url_for)
        body = template.render(context)
        
        resp_headers = [(b"content-type", b"text/html; charset=utf-8")]
        if headers:
            resp_headers.extend(headers)
            
        return {
            "status": status_code,
            "headers": resp_headers,
            "body": body.encode("utf-8"),
        }
