---
title: Testing Client API
description: Reference for the in-process HTTP test client.
---

## `JustAPITestClient`

The test client sends HTTP requests to your application without starting a real server, using `tokio` duplex streams.

```python
from justapi import JustAPITestClient

client = JustAPITestClient(app)
```

### Methods

```python
response = client.get("/items/42")
response = client.post("/items/", body={"name": "Item"})
response = client.put("/items/42", body={"name": "Updated"})
response = client.patch("/items/42", body={"name": "Patched"})
response = client.delete("/items/42")
```

### Response Object

| Attribute | Type | Description |
|---|---|---|
| `status` | `int` | HTTP status code |
| `body` | `bytes` | Raw response body |
| `json()` | `dict` | Parsed JSON body |

### Example Test

```python
from justapi import JustAPIApp, JustAPITestClient

app = JustAPIApp()

@app.get("/ping")
def ping(request):
    return {"status": "ok"}

client = JustAPITestClient(app)
response = client.get("/ping")
assert response.status == 200
assert response.json() == {"status": "ok"}
```

## See Also

- [Testing Guide](/contributing/testing-guide/) — Writing tests for JustAPI
- [JustAPIApp](/api-reference/justapiapp/) — App configuration
