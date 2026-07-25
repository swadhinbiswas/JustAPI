---
title: Advanced Security
description: OAuth2 scopes, HTTP Basic Auth, and API key authentication in JustAPI.
keywords: [JustAPI, OAuth2 scopes, HTTP Basic Auth, API keys, advanced security]
---

## OAuth2 Scopes

Control access with fine-grained permissions:

```python
from justapi import JustAPIApp, Security
from fastapi.security import OAuth2PasswordBearer

oauth2_scheme = OAuth2PasswordBearer(
    tokenUrl="token",
    scopes={"read": "Read access", "write": "Write access", "admin": "Admin access"},
)

@app.get("/items/")
def list_items(token: str = Security(oauth2_scheme, scopes=["read"])):
    return []

@app.post("/items/")
def create_item(token: str = Security(oauth2_scheme, scopes=["write"])):
    return {}

@app.delete("/items/{item_id}")
def delete_item(item_id: int, token: str = Security(oauth2_scheme, scopes=["admin"])):
    return {"deleted": item_id}
```

## HTTP Basic Auth

```python
from fastapi import Depends, HTTPException, status
from fastapi.security import HTTPBasic, HTTPBasicCredentials
import secrets

security = HTTPBasic()

def verify_credentials(credentials: HTTPBasicCredentials = Depends(security)):
    correct_username = secrets.compare_digest(credentials.username, "admin")
    correct_password = secrets.compare_digest(credentials.password, "secret")
    if not (correct_username and correct_password):
        raise HTTPException(status_code=401, detail="Invalid credentials")
    return credentials.username

@app.get("/admin")
def admin_panel(username: str = Depends(verify_credentials)):
    return {"message": f"Welcome {username}"}
```

## API Key Authentication

```python
from fastapi import Security
from fastapi.security import APIKeyHeader

api_key_header = APIKeyHeader(name="X-API-Key")

def verify_api_key(api_key: str = Security(api_key_header)):
    if api_key != "my-secret-key":
        raise HTTPException(status_code=403, detail="Invalid API key")
    return api_key

@app.get("/data")
def get_data(key: str = Security(verify_api_key)):
    return {"data": "sensitive"}
```

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic security
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — JWT implementation
- [Secure Configuration](/security/secure-configuration/) — production security
