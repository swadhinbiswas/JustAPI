---
title: First Steps with JustAPI
description: Create your first API endpoint in under 2 minutes.
---

## 1. Create a `main.py` File

```python
from justapi import JustAPIApp

app = JustAPIApp(title="My First App", version="1.0.0")

@app.get("/")
def read_root(request):
    return {"status": "ok", "message": "Welcome to JustAPI!"}

@app.get("/users/{user_id}")
def read_user(request, user_id: int):
    return {"user_id": user_id, "active": True}
```

## 2. Run the Application

Run directly using Python:

```bash
python main.py
```

Or run using the hot-reloading dev server:

```bash
justapi serve --reload
```

## 3. Explore Interactive Documentation

Open your browser to:
* **Swagger UI:** `http://127.0.0.1:8000/docs`
* **ReDoc:** `http://127.0.0.1:8000/redoc`
* **Scalar UI:** `http://127.0.0.1:8000/scalar`
* **OpenAPI Spec:** `http://127.0.0.1:8000/openapi.json`
