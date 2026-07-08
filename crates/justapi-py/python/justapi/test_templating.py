import pytest
import os
from justapi import Jinja2Templates, JustAPIApp, JustAPITestClient

def test_jinja2_templates(tmp_path):
    templates_dir = tmp_path / "templates"
    templates_dir.mkdir()
    
    index_html = templates_dir / "index.html"
    index_html.write_text("Hello {{ name }}!")

    app = JustAPIApp()
    templates = Jinja2Templates(directory=str(templates_dir))

    @app.get("/hello")
    def hello(request):
        return templates.TemplateResponse("index.html", {"name": "World"})
        
    client = JustAPITestClient(app)
    response = client.get("/hello")
    assert response["status"] == 200
    assert response["body"] == b"Hello World!"
    # Check headers
    assert response["headers"]["content-type"] == "text/html; charset=utf-8"
