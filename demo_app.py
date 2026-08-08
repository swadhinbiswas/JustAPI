import asyncio
from justapi import JustAPIApp
from pydantic import BaseModel
from typing import Optional

# 1. Initialize the App
app = JustAPIApp()

# 2. Add some Middleware (Auth, CORS, Security)
# (assuming middleware plugin system is configured)

# 3. Request Validation Models
class ItemRequest(BaseModel):
    name: str
    price: float
    description: Optional[str] = None

# 4. Standard Async Route (High Concurrency)
@app.get("/async-demo")
async def async_demo():
    await asyncio.sleep(0.01) # Simulate DB or IO delay
    return {"status": "success", "message": "Handled without blocking the Tokio loop!"}

# 5. Native CPU-bound Route
@app.get("/cpu-heavy")
def cpu_heavy():
    # Will be offloaded natively via spawn_blocking
    total = sum(i * i for i in range(10000))
    return {"total": total}

# 6. JSON Validation & Serialization Route
@app.post("/items")
async def create_item(item: ItemRequest):
    # 'item' is fully validated in Rust before Python executes!
    return {"message": "Item created successfully", "data": item.model_dump()}

# 7. gRPC Integration Mock Route (assuming defined protobuf)
@app.get("/grpc-status")
def grpc_status():
    return {"grpc_port": 50051, "status": "running concurrently"}

# 8. Start the Application
if __name__ == "__main__":
    print("🚀 Starting JustAPI Demo Application on http://127.0.0.1:8080")
    print("Try the following endpoints:")
    print("  - GET  http://127.0.0.1:8080/async-demo")
    print("  - GET  http://127.0.0.1:8080/cpu-heavy")
    print("  - POST http://127.0.0.1:8080/items (Body: {\"name\": \"Test\", \"price\": 99.99})")
    
    app.run("127.0.0.1:8080")
