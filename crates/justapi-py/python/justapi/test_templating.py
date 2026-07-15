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


def test_template_url_for(tmp_path):
    templates_dir = tmp_path / "templates"
    templates_dir.mkdir()
    (templates_dir / "page.html").write_text(
        '<a href="{{ url_for("item-detail", item_id=item_id) }}">link</a>'
    )

    app = JustAPIApp()
    templates = Jinja2Templates(directory=str(templates_dir))

    @app.get("/items/{item_id}", name="item-detail")
    def item(request, item_id: int):
        return templates.TemplateResponse(
            "page.html", {"request": request, "item_id": item_id}
        )

    client = JustAPITestClient(app)
    resp = client.get("/items/42")
    assert resp["status"] == 200
    assert b'href="/items/42"' in resp["body"]

