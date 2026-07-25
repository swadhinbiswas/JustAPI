---
title: Simple OAuth2 with Password and Bearer
description: Implement password-based OAuth2 authentication in JustAPI.
keywords: [JustAPI, OAuth2, password, bearer, authentication, security]
---

## Basic OAuth2 Setup

```python
from justapi import JustAPIApp, Depends, HTTPException
from fastapi.security import OAuth2PasswordBearer

app = JustAPIApp()

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

# Fake user database
fake_users_db = {
    "alice": {"user_id": 1, "name": "Alice", "password": "secret"},
}

def authenticate_user(username: str, password: str):
    user = fake_users_db.get(username)
    if user and user["password"] == password:
        return user
    return None

@app.post("/token")
def login(username: str, password: str):
    user = authenticate_user(username, password)
    if not user:
        raise HTTPException(status_code=401, detail="Invalid credentials")
    return {"access_token": "fake-jwt-token", "token_type": "bearer"}
```

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic setup
- [Get Current User](/tutorials/security/get-current-user/) — extracting user
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — production JWT
