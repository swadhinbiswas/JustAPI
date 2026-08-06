---
title: OAuth2 with Password Hashing and JWT Tokens
description: Implement production-ready OAuth2 with password hashing and JWT tokens in JustAPI.
keywords: [JustAPI, OAuth2, JWT, password hashing, security, authentication]
---

## Production OAuth2 with JWT

```python
import jwt
from datetime import datetime, timedelta
from justapi import JustAPIApp, Depends, HTTPException
from justapi.auth import OAuth2PasswordBearer
from passlib.context import CryptContext

SECRET_KEY = "your-secret-key"
ALGORITHM = "HS256"
ACCESS_TOKEN_EXPIRE_MINUTES = 30

app = JustAPIApp()
oauth2_scheme = OAuth2PasswordBearer(tokenUrl="token")
pwd_context = CryptContext(schemes=["bcrypt"], deprecated="auto")

def verify_password(plain: str, hashed: str) -> bool:
    return pwd_context.verify(plain, hashed)

def hash_password(password: str) -> str:
    return pwd_context.hash(password)

def create_access_token(data: dict) -> str:
    to_encode = data.copy()
    expire = datetime.utcnow() + timedelta(minutes=ACCESS_TOKEN_EXPIRE_MINUTES)
    to_encode.update({"exp": expire})
    return jwt.encode(to_encode, SECRET_KEY, algorithm=ALGORITHM)

def get_current_user(token: str = Security(oauth2_scheme)):
    try:
        payload = jwt.decode(token, SECRET_KEY, algorithms=[ALGORITHM])
        user_id = payload.get("user_id")
        if user_id is None:
            raise HTTPException(status_code=401)
        return {"user_id": user_id}
    except jwt.PyJWTError:
        raise HTTPException(status_code=401, detail="Invalid token")

@app.post("/token")
def login(username: str, password: str):
    # Verify credentials and return JWT
    token = create_access_token({"user_id": 1, "sub": username})
    return {"access_token": token, "token_type": "bearer"}

@app.get("/users/me")
def read_me(current_user: dict = Security(get_current_user)):
    return current_user
```

## See Also

- [Security — First Steps](/tutorials/security/first-steps/) — basic setup
- [Get Current User](/tutorials/security/get-current-user/) — extracting user
- [Simple OAuth2](/tutorials/security/simple-oauth2/) — simpler setup
