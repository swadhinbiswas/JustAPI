# Migrating from FastAPI

JustAPI aims for high compatibility with FastAPI. In most cases, migration is as simple as changing imports.

## Import Changes

```python
# FastAPI
from fastapi import FastAPI, Depends, HTTPException

# JustAPI
from justapi import JustAPIApp, Depends, HTTPException
```

## The Application Object

Replace `FastAPI()` with `JustAPIApp()`:

```python
# FastAPI
app = FastAPI()

# JustAPI
app = JustAPIApp()
```

## Middlewares

Middleware in JustAPI acts exactly like FastAPI middlewares using the `http` interceptor:

```python
@app.middleware("http")
async def process_time_header(request, call_next):
    # Your logic
    response = await call_next(request)
    return response
```

**Ordering (same as Starlette/FastAPI):** the **last** middleware added is the
outermost — it sees the request first and the response last. (Before v2.1.0
JustAPI ran the first-added middleware outermost, the opposite; reverse your
registrations if you relied on that.)

## Pydantic Models

Body and form models work the same way:

```python
from pydantic import BaseModel

class Item(BaseModel):
    name: str
    price: float

@app.post("/items")
def create_item(item: Item):          # JSON body → validated Item
    return item.model_dump()

@app.post("/login")
def login(form: LoginForm = Form(...)):   # multipart or urlencoded form → model
    return {"user": form.username}
```

Validation errors return `422`; `@app.exception_handler(RequestValidationError)`
handlers receive an exception whose `.errors()` returns the error list (the
attribute `exc.errors` works too).

## Production Process Management

`justapi serve` runs Python apps with production controls:

```bash
justapi serve --addr 0.0.0.0:8080 --workers 4   # prefork (SO_REUSEPORT), respawn
justapi serve --addr 127.0.0.1:8000 --reload    # restart on *.py change
```

## Performance Differences

Once you switch, you should immediately notice a dramatic drop in CPU overhead for routing and connection handling, translating to much higher throughput under load.
