---
title: Authentication
description: JWT, OAuth2, and API key authentication in JustAPI — Rust-native and Python-compatible.
keywords: [JustAPI, auth, JWT, OAuth2, API key, authentication, security]
---

## JwtAuth (Rust-native)

Fast, Rust-native JWT encoding/decoding:

```python
from justapi import JustAPIApp, JwtAuth, Security

app = JustAPIApp()

jwt = JwtAuth(secret="your-secret-key", algorithm="HS256")

@app.get("/protected")
def protected(token: dict = Security(jwt)):
    return {"user": token["sub"]}
```

### JwtAuth Options

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `secret` | `str` | — | Signing secret |
| `algorithm` | `str` | `"HS256"` | JWT algorithm |
| `auto_error` | `bool` | `True` | Raise 401 on invalid token |

### Supported Algorithms

HS256, HS384, HS512, RS256, RS384, RS512, ES256, ES384, ED25519

### Encode/Decode

```python
# Encode
token = jwt.encode({"sub": "alice", "role": "admin"})

# Decode
claims = jwt.decode(token, verify_exp=True, verify_iat=True)
```

### Decode Options

| Option | Type | Description |
|--------|------|-------------|
| `verify_exp` | `bool` | Verify expiration claim |
| `verify_iat` | `bool` | Verify issued-at claim |
| `verify_nbf` | `bool` | Verify not-before claim |
| `verify_aud` | `bool` | Verify audience claim |
| `verify_iss` | `bool` | Verify issuer claim |

## OAuth2PasswordBearer

FastAPI-compatible bearer token extractor:

```python
from justapi import OAuth2PasswordBearer

oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")

@app.get("/users/me")
def read_me(token: str = Security(oauth2_scheme)):
    return {"token": token}
```

## OAuth2PasswordRequestForm

Form dependency for token endpoints:

```python
from justapi import OAuth2PasswordRequestForm

@app.post("/token")
def login(form: OAuth2PasswordRequestForm):
    # form.username, form.password, form.scope
    return {"access_token": form.username, "token_type": "bearer"}
```

### Strict Variant

```python
from justapi import OAuth2PasswordRequestFormStrict

# Requires grant_type=password in the form
@app.post("/token")
def login(form: OAuth2PasswordRequestFormStrict):
    ...
```

## Set JWT Auth (Convenience)

```python
app.set_jwt_auth(secret="my-secret", algorithm="HS256")
```

This adds Rust-native JWT middleware to all routes.

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic auth setup
- [OAuth2 + JWT](/tutorials/security/oauth2-jwt/) — production JWT
- [Advanced Security](/advanced/advanced-security/) — scopes, API keys
