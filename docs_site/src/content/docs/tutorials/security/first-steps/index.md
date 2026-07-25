---
title: Security — First Steps
description: Add basic authentication to a JustAPI route.
keywords: [JustAPI, security, authentication, OAuth2, bearer token]
---

## Adding Security to a Route

Use `Security()` (like `Depends()` but for security) to protect routes:

```python
from justapi import JustAPIApp, Security, Depends
from fastapi.security import OAuth2PasswordBearer

app = JustAPIApp()

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

def get_current_user(token: str = Security(oauth2_scheme)):
    # In production, decode the token
    return {"user_id": 1, "name": "Alice"}

@app.get("/users/me")
def read_users_me(current_user: dict = Security(get_current_user)):
    return current_user
```

## OAuth2PasswordBearer

The `tokenUrl` tells the docs where to send login requests. It appears as an "Authorize" button in Swagger UI.

## See Also

- [Get Current User](/tutorials/security/get-current-user/) — extracting user from token
- [Simple OAuth2](/tutorials/security/simple-oauth2/) — password-based OAuth2
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — full JWT implementation
