---
title: Form Models
description: Use Pydantic models for form data validation in JustAPI.
keywords: [JustAPI, form models, Pydantic forms, form validation]
---

## Pydantic Models for Forms

Use `Body(..., embed=True)` with Pydantic models for complex form data:

```python
from pydantic import BaseModel
from justapi import JustAPIApp, Form

class LoginForm(BaseModel):
    username: str
    password: str
    remember_me: bool = False

app = JustAPIApp()

@app.post("/login")
def login(form: LoginForm = Form(...)):
    return {"username": form.username, "remember_me": form.remember_me}
```

:::tip
When using Pydantic models with `Form(...)`, the `embed=True` parameter is required by Pydantic to correctly parse the form fields.
:::

## File + Form Combination

```python
from justapi import JustAPIApp, Form, UploadFile

class CreateItemWithImage(BaseModel):
    name: str
    description: str = ""

app = JustAPIApp()

@app.post("/items/")
async def create_item(
    item: CreateItemWithImage = Form(...),
    image: UploadFile = File(...),
):
    return {"item": item.model_dump(), "filename": image.filename}
```

## See Also

- [Form Data](/tutorials/form-data/) — basic form fields
- [Request Files](/tutorials/request-files/) — file uploads
- [Request Body](/tutorials/request-body/) — JSON request bodies
