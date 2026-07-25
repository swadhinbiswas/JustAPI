---
title: Sub-dependencies
description: Chain dependencies in JustAPI where one dependency requires another.
keywords: [JustAPI, sub-dependencies, dependency chain, dependency injection]
---

## Chaining Dependencies

Dependencies can depend on other dependencies. JustAPI resolves the entire chain automatically:

```python
from justapi import JustAPIApp, Depends

app = JustAPIApp()

# Level 1: get token from header
def get_token(authorization: str = Header(...)):
    return authorization.replace("Bearer ", "")

# Level 2: decode token to get user
def get_user(token: str = Depends(get_token)):
    # In production, decode JWT
    return {"user_id": 1, "name": "Alice"}

# Level 3: use user in handler
@app.get("/profile")
def profile(user: dict = Depends(get_user)):
    return {"user": user["name"]}
```

JustAPI resolves `get_token` → `get_user` → `profile` automatically.

## Sharing Dependencies

```python
def get_db():
    return DatabaseConnection()

def get_current_user(db: DBSession = Depends(get_db)):
    return db.query_user()

def get_admin_user(user: dict = Depends(get_current_user)):
    if user.get("role") != "admin":
        raise HTTPException(status_code=403)
    return user
```

## See Also

- [Classes as Dependencies](/tutorials/dependencies/classes-as-dependencies/) — class-based deps
- [Global Dependencies](/tutorials/dependencies/global-dependencies/) — app-wide deps
