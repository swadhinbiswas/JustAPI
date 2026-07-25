---
title: Advanced Dependencies
description: Parameterized dependencies, async generator dependencies, and dependency overrides in JustAPI.
keywords: [JustAPI, advanced dependencies, parameterized, overrides, async generator]
---

## Parameterized Dependencies

Create dependencies that accept arguments:

```python
from justapi import JustAPIApp, Depends

def get_query_filter(field: str, value: str):
    def dependency():
        return {field: value}
    return dependency

app = JustAPIApp()

@app.get("/items/")
def list_items(fresh: str = Depends(get_query_filter("status", "active"))):
    return {"filter": fresh}
```

## Dependency Overrides

Override dependencies for testing:

```python
def get_db():
    return ProductionDB()

app = JustAPIApp(dependencies=[Depends(get_db)])

# In tests
def override_get_db():
    return TestDB()

app.dependency_overrides[get_db] = override_get_db

def test_handler():
    response = client.get("/items/")
    assert response.status == 200
```

## Async Generator Dependencies

```python
async def get_async_db():
    db = await create_async_connection()
    try:
        yield db
    finally:
        await db.close()

@app.get("/items/")
async def list_items(db = Depends(get_async_db)):
    items = await db.fetch_all("SELECT * FROM items")
    return {"items": items}
```

## See Also

- [Dependencies with yield](/tutorials/dependencies/dependencies-with-yield/) — cleanup patterns
- [Testing with Overrides](/advanced/async-tests/) — testing dependencies
- [Classes as Dependencies](/tutorials/dependencies/classes-as-dependencies/) — class-based deps
