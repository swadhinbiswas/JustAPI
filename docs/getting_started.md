# Getting Started

This guide will walk you through building your first web application with JustAPI.

## Installation

Install JustAPI using `pip`:

```bash
pip install justapi
```

We also recommend installing `pydantic` for data validation:

```bash
pip install pydantic
```

## Your First Application

Create a file named `main.py` and add the following code:

```python
from justapi import JustAPIApp
from pydantic import BaseModel

# Initialize the application
app = JustAPIApp()

# Define a Pydantic schema for validation
class Item(BaseModel):
    name: str
    price: float
    is_offer: bool = None

# Create a basic GET route
@app.get("/")
async def read_root():
    return {"Hello": "World"}

# Create a route with path parameters
@app.get("/items/{item_id}")
async def read_item(item_id: int, q: str = None):
    return {"item_id": item_id, "q": q}

# Create a POST route with body validation
@app.post("/items/")
async def create_item(item: Item):
    return {"item_name": item.name, "item_price": item.price}

if __name__ == "__main__":
    # Start the high-performance Rust server
    app.run("127.0.0.1:8000")
```

## Running the Server

Run your application from the terminal:

```bash
python main.py
```

You should see output indicating that the Rust Tokio runtime has initialized and the server is listening on `127.0.0.1:8000`.

## Testing the API

You can test your API using tools like `curl`:

**GET Request:**
```bash
curl http://127.0.0.1:8000/
# Output: {"Hello": "World"}
```

**GET Request with parameters:**
```bash
curl "http://127.0.0.1:8000/items/5?q=somequery"
# Output: {"item_id": 5, "q": "somequery"}
```

**POST Request with JSON body:**
```bash
curl -X POST http://127.0.0.1:8000/items/ \
  -H "Content-Type: application/json" \
  -d '{"name": "Laptop", "price": 999.99}'
# Output: {"item_name": "Laptop", "item_price": 999.99}
```

## Next Steps

Now that you have a basic application running, check out the [API Reference](api_reference.md) to explore routing, dependency injection, and advanced features.
