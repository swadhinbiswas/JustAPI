---
title: Get Current User
description: Extract and validate the current authenticated user from a JWT token in JustAPI.
keywords: [JustAPI, security, current user, JWT, token]
---

## Extracting User from Token

```python
from justapi import JustAPIApp, Security
from justapi.auth import OAuth2PasswordBearer

app = JustAPIApp()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

async def get_current_user(token: str = Security(oauth2_scheme)):
    # Decode JWT and return user
    user = decode_token(token)
    if user is None:
        raise HTTPException(status_code=401, detail="Invalid token")
    return user

@app.get("/users/me")
def read_me(current_user: dict = Security(get_current_user)):
    return current_user
```

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic security setup
- [Simple OAuth2](/tutorials/security/simple-oauth2/) — password-based auth
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — full JWT implementation
