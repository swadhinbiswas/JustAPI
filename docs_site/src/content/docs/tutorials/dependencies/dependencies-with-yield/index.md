---
title: Dependencies with yield
description: Use yield dependencies in JustAPI for setup/teardown patterns like database connections.
keywords: [JustAPI, dependencies, yield, setup, teardown, cleanup]
---

## Setup/Teardown with yield

Use `yield` in dependencies to perform cleanup after the request:

```python
from justapi import JustAPIApp, Depends

def get_db_session():
    session = create_session()
    try:
        yield session
    finally:
        session.close()

app = JustAPIApp()

@app.get("/users/")
def list_users(db = Depends(get_db_session)):
    users = db.query("SELECT * FROM users")
    return {"users": users}
```

The code before `yield` runs before the request. The code after `yield` runs after the request (even if it raises an exception).

## Database Connection Pool

```python
def get_db():
    db = DBConnection("sqlite:///app.db")
    try:
        yield db
    finally:
        db.disconnect()
```

## Multiple Cleanup Steps

```python
def get_resource():
    resource = acquire_resource()
    try:
        yield resource
    finally:
        release_resource(resource)
        cleanup_temp_files()
```

## See Also

- [SQL Databases](/tutorials/database-integration/) — database integration
- [Classes as Dependencies](/tutorials/dependencies/classes-as-dependencies/) — class-based deps
