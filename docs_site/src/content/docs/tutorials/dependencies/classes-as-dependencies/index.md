---
title: Classes as Dependencies
description: Use Python classes as dependencies in JustAPI for encapsulated logic with state.
keywords: [JustAPI, dependencies, classes, dependency injection]
---

## Callable Dependencies as Classes

Instead of plain functions, use classes as dependencies when you need to encapsulate state:

```python
from justapi import JustAPIApp, Depends

class PaginationParams:
    def __init__(self, skip: int = 0, limit: int = 10):
        self.skip = skip
        self.limit = limit
        self.offset = skip * limit  # computed property

@app.get("/items/")
def list_items(pagination: PaginationParams = Depends()):
    return {"skip": pagination.skip, "limit": pagination.limit, "offset": pagination.offset}
```

JustAPI calls `PaginationParams(skip=..., limit=...)` automatically — the parameters come from query params.

## Class with Dependencies

```python
class DBSession:
    def __init__(self, db_url: str):
        self.db_url = db_url
        # In production, create connection pool
    
    def execute(self, query: str):
        return []

def get_db():
    return DBSession("sqlite:///app.db")

@app.get("/users/")
def list_users(db: DBSession = Depends(get_db)):
    return db.execute("SELECT * FROM users")
```

## See Also

- [Sub-dependencies](/tutorials/dependencies/sub-dependencies/) — dependencies of dependencies
- [Dependencies with yield](/tutorials/dependencies/dependencies-with-yield/) — cleanup patterns
