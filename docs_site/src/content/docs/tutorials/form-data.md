---
title: Form Data
description: Handle HTML form submissions and form data in JustAPI.
keywords: [JustAPI, form data, form submission, HTML forms]
---

## Basic Form Data

Use `Form(...)` to receive form data:

```python
from justapi import JustAPIApp, Form

app = JustAPIApp()

@app.post("/login")
def login(username: str = Form(...), password: str = Form(...)):
    return {"username": username}
```

This accepts `application/x-www-form-urlencoded` requests.

## Required vs Optional

```python
@app.post("/items")
def create_item(
    name: str = Form(...),           # required
    description: str = Form(None),   # optional
):
    return {"name": name, "description": description}
```

## Multiple Form Fields

```python
@app.post("/submit")
def submit_form(
    title: str = Form(...),
    body: str = Form(...),
    tags: str = Form(""),
    publish: bool = Form(False),
):
    return {"title": title, "body": body, "tags": tags, "publish": publish}
```

## See Also

- [Form Models](/tutorials/form-models/) — Pydantic models for forms
- [Request Files](/tutorials/request-files/) — file uploads
- [Request Body](/tutorials/request-body/) — JSON request bodies
