---
title: Templates (Jinja2)
description: Render Jinja2 templates for server-side HTML in JustAPI.
keywords: [JustAPI, templates, Jinja2, HTML, server-side rendering]
---

## Basic Template Rendering

Install dependencies:

```bash
pip install jinja2
```

Create a template directory:

```
templates/
  index.html
  items.html
```

Set up templates in your app:

```python
from justapi import JustAPIApp
from justapi import Jinja2Templates

app = JustAPIApp()
templates = Jinja2Templates(directory="templates")

@app.get("/")
def index(request):
    return templates.TemplateResponse("index.html", {"request": request, "title": "Home"})
```

## Template Example

```html
<!-- templates/index.html -->
<!DOCTYPE html>
<html>
<head>
    <title>{{ title }}</title>
</head>
<body>
    <h1>{{ title }}</h1>
    <p>Welcome to JustAPI</p>
</body>
</html>
```

## Passing Data

```python
@app.get("/items")
def list_items(request):
    items = [{"id": 1, "name": "Widget"}, {"id": 2, "name": "Gadget"}]
    return templates.TemplateResponse("items.html", {
        "request": request,
        "items": items,
        "title": "Items",
    })
```

## See Also

- [Frontend](/tutorials/) — frontend integration
- [Static Files](/tutorials/static-files/) — serving CSS/JS
- [Custom Response Classes](/advanced/custom-response/) — HTML responses
