---
title: Advanced Security
description: OAuth2 scopes, HTTP Basic Auth, and API key authentication in JustAPI.
keywords: [JustAPI, OAuth2 scopes, HTTP Basic Auth, API keys, advanced security]
---

## OAuth2 Bearer Tokens

Protect routes with the `OAuth2PasswordBearer` dependency from `justapi.auth`
(extracts the `Authorization: Bearer <token>` header and 401s when missing):

```python
from justapi import JustAPIApp, Security
from justapi.auth import OAuth2PasswordBearer

app = JustAPIApp()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@app.get("/items/")
def list_items(token: str = Security(oauth2_scheme)):
    return []

@app.post("/items/")
def create_item(token: str = Security(oauth2_scheme)):
    return {}
```

For role-based access control, chain dependencies — the `Security` marker is
`Depends` with OAuth2-scope metadata, and scope/role enforcement is your
dependency's job:

```python
from justapi import Depends, Security, HTTPException

def require_admin(token: str = Depends(oauth2_scheme)):
    if token != "admin-token":
        raise HTTPException(status_code=403, detail="Admin access required")
    return token

@app.delete("/items/{item_id}")
def delete_item(item_id: int, admin: str = Security(require_admin)):
    return {"deleted": item_id}
```

## HTTP Basic Auth

Use the `Header` parameter type and a dependency for basic credentials:

```python
from justapi import Depends, HTTPException, Header
import base64
import secrets

def verify_basic(authorization: str = Header(None)):
    if not authorization or not authorization.startswith("Basic "):
        raise HTTPException(status_code=401, detail="Not authenticated")
    try:
        decoded = base64.b64decode(authorization[6:]).decode()
        username, _, password = decoded.partition(":")
    except Exception:
        raise HTTPException(status_code=401, detail="Invalid credentials")
    correct_user = secrets.compare_digest(username, "admin")
    correct_pass = secrets.compare_digest(password, "secret")
    if not (correct_user and correct_pass):
        raise HTTPException(status_code=401, detail="Invalid credentials")
    return username

@app.get("/admin")
def admin_panel(username: str = Depends(verify_basic)):
    return {"message": f"Welcome {username}"}
```

## API Key Authentication

API keys are just a header — validate with a `Header` dependency:

```python
from justapi import Depends, HTTPException, Header

def verify_api_key(x_api_key: str = Header(None)):
    if x_api_key != "my-secret-key":
        raise HTTPException(status_code=403, detail="Invalid API key")
    return x_api_key

@app.get("/data")
def get_data(key: str = Depends(verify_api_key)):
    return {"data": "sensitive"}
```

## Rust-Native JWT

For production JWT, the Rust-native `app.set_jwt_auth(secret=...)` middleware
validates every request's `Authorization` header in Rust (no GIL), or use
`JwtAuth` as a `Depends` for per-route control — see the
[Auth API](/api-reference/auth/).

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic security
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — JWT implementation
- [Secure Configuration](/security/secure-configuration/) — production security
- [Auth API](/api-reference/auth/) — `JwtAuth`, `OAuth2PasswordBearer`, form dependencies
