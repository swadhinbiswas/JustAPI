import time
from justapi import JustAPIApp

app = JustAPIApp()

@app.middleware("http")
async def add_process_time_header(request, call_next):
    start_time = time.time()
    
    # Process the request down the chain
    response = await call_next(request)
    
    # Add a custom header
    process_time = time.time() - start_time
    if "headers" not in response:
        response["headers"] = []
    response["headers"].append((b"X-Process-Time", str(process_time).encode("utf-8")))
    
    return response

@app.get("/")
async def root():
    # Simulate some work
    time.sleep(0.1)
    return {"message": "Hello World with Middleware"}

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
