import asyncio
from justapi import JustAPIApp
from pydantic import BaseModel

app = JustAPIApp()

# Mock database
fake_items_db = [{"item_name": "Foo"}, {"item_name": "Bar"}, {"item_name": "Baz"}]

class Item(BaseModel):
    item_name: str

@app.post("/items/")
async def create_item(item: Item):
    """
    Simulates inserting a record into a database asynchronously.
    """
    await asyncio.sleep(0.5) # Simulate DB latency
    fake_items_db.append(item.model_dump())
    return item

@app.get("/items/")
async def read_items(skip: int = 0, limit: int = 10):
    """
    Simulates querying a database.
    """
    await asyncio.sleep(0.1) # Simulate DB latency
    return fake_items_db[skip : skip + limit]

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
