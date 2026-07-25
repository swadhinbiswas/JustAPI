---
title: Security — First Steps
description: Add authentication to a JustAPI route using OAuth2 and Security dependencies.
keywords: [JustAPI, security, OAuth2, authentication, bearer token, password flow]
---

This tutorial builds a simple authentication system step by step. By the end you'll understand the full OAuth2 password flow and how to protect routes.

## The Problem

You need to protect a route so only authenticated users can access it. Unauthenticated requests should get a `401` error.

## Step 1: Add a Security Dependency

```python
from justapi import JustAPIApp, Security
from fastapi.security import OAuth2PasswordBearer

app = JustAPIApp()

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@app.get("/users/me")
def read_users_me(token: str = Security(oauth2_scheme)):
    return {"token": token}
```

### What happens here?

1. `OAuth2PasswordBearer(tokenUrl="token")` creates a dependency that expects an `Authorization: Bearer <token>` header.
2. `Security(oauth2_scheme)` tells JustAPI this is a security requirement — it appears in the OpenAPI docs with an "Authorize" button.
3. The `token` parameter receives the raw token string from the header.

If the request has no `Authorization` header, JustAPI returns a `401` error automatically — no code needed.

### Try it

Start the server and go to `http://localhost:8000/docs`. You'll see an **Authorize** button at the top.

Click it, enter any value, and then call `/users/me`. The request includes the header automatically.

Without the header:

```json
{
  "detail": "Not authenticated"
}
```

## Step 2: Validate the Token

The token is just a string. We need to verify it:

```python
from justapi import JustAPIApp, Security, HTTPException
from fastapi.security import OAuth2PasswordBearer

app = JustAPIApp()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

fake_users_db = {
    "alice": {"user_id": 1, "name": "Alice"},
    "bob": {"user_id": 2, "name": "Bob"},
}

def get_current_user(token: str = Security(oauth2_scheme)):
    if token not in fake_users_db:
        raise HTTPException(status_code=401, detail="Invalid token")
    return fake_users_db[token]

@app.get("/users/me")
def read_users_me(current_user: dict = Security(get_current_user)):
    return current_user
```

Now the flow is:

1. Client sends `Authorization: Bearer alice`
2. `oauth2_scheme` extracts `"alice"` as the token
3. `get_current_user` looks up `"alice"` in the database
4. If found, returns the user dict — which becomes `current_user` in the handler
5. If not found, raises `401`

:::note
The token `"alice"` is the actual value after `Bearer` in the header. In production this would be a JWT or opaque token.
:::

## Step 3: Add a Token Endpoint

The frontend needs a way to log in and get a token. Create a `/token` endpoint:

```python
from justapi import JustAPIApp, Security, HTTPException
from fastapi.security import OAuth2PasswordBearer
from pydantic import BaseModel

app = JustAPIApp()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

class TokenRequest(BaseModel):
    username: str
    password: str

fake_users_db = {
    "alice": {"user_id": 1, "name": "Alice", "password": "secret123"},
    "bob": {"user_id": 2, "name": "Bob", "password": "pass456"},
}

def get_current_user(token: str = Security(oauth2_scheme)):
    if token not in fake_users_db:
        raise HTTPException(status_code=401, detail="Invalid token")
    return fake_users_db[token]

@app.post("/token")
def login(request: TokenRequest):
    user = fake_users_db.get(request.username)
    if not user or user["password"] != request.password:
        raise HTTPException(status_code=401, detail="Wrong username or password")
    return {"access_token": request.username, "token_type": "bearer"}

@app.get("/users/me")
def read_users_me(current_user: dict = Security(get_current_user)):
    return current_user
```

### The full flow

1. Client sends `POST /token` with `{"username": "alice", "password": "secret123"}`
2. Server returns `{"access_token": "alice", "token_type": "bearer"}`
3. Client sends `GET /users/me` with header `Authorization: Bearer alice`
4. Server returns `{"user_id": 1, "name": "Alice"}`

:::tip
In the Swagger UI (`/docs`), you can now click **Authorize**, enter `alice` / `secret123`, and all subsequent requests will include the token automatically.
:::

## Step 4: Add User Roles

Extend the dependency to check permissions:

```python
def get_current_user(token: str = Security(oauth2_scheme)):
    if token not in fake_users_db:
        raise HTTPException(status_code=401, detail="Invalid token")
    return fake_users_db[token]

def require_admin(current_user: dict = Security(get_current_user)):
    if current_user.get("role") != "admin":
        raise HTTPException(status_code=403, detail="Admin access required")
    return current_user

@app.get("/admin/dashboard")
def admin_dashboard(admin: dict = Security(require_admin)):
    return {"message": f"Welcome admin {admin['name']}"}
```

Now `/admin/dashboard` requires both a valid token AND admin role.

## What JustAPI Does Under the Hood

When you use `Security(oauth2_scheme)`:

1. **Before the request**: JustAPI checks for the `Authorization` header
2. **Missing header**: Returns `401` with `{"detail": "Not authenticated"}`
3. **Invalid header format**: Returns `401` with details about the expected format
4. **Valid header**: Extracts the token and passes it to your dependency
5. **In the docs**: Adds an "Authorize" button to Swagger UI

## Recap

- `OAuth2PasswordBearer(tokenUrl="token")` creates the auth dependency
- `Security()` marks it as a security requirement in the docs
- Your dependency validates the token and returns the user
- Unauthenticated requests get a `401` automatically
- Chain dependencies for role-based access control

## See Also

- [Get Current User](/tutorials/security/get-current-user/) — extracting user from token
- [Simple OAuth2](/tutorials/security/simple-oauth2/) — password-based OAuth2
- [OAuth2 + JWT Tokens](/tutorials/security/oauth2-jwt/) — production JWT implementation
- [Advanced Security](/advanced/advanced-security/) — OAuth2 scopes, API keys
