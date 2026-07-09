import asyncio
import httpx
from justapi import JustAPIApp, APIRouter

app = JustAPIApp()
router = APIRouter(prefix="/router")

@app.middleware("http")
async def add_process_time_header(request, call_next):
    assert request.method == "GET"
    assert request.path.startswith("/test") or request.path.startswith("/router")
    
    response = await call_next(request)
    
    if "headers" not in response:
        response["headers"] = []
    
    response["headers"].append((b"X-Process-Time", b"0.123"))
    return response

@app.get("/test_async")
async def get_test_async():
    return {"message": "async success"}

async def route_middleware(request, call_next):
    res = await call_next(request)
    res["headers"].append((b"X-Route-Mw", b"route"))
    return res

@app.get("/test_route_mw", middlewares=[route_middleware])
def get_test_route_mw():
    return {"message": "route mw"}

async def router_middleware(request, call_next):
    res = await call_next(request)
    res["headers"].append((b"X-Router-Mw", b"router"))
    return res

router.middlewares.append(router_middleware)

@router.get("/test", middlewares=[route_middleware])
def get_router_test():
    return {"message": "router mw"}

app.include_router(router)

@app.get("/test_sync")
def get_test_sync():
    return {"message": "sync success"}

async def main():
    import threading
    import time
    
    def run_server():
        app.run("127.0.0.1:8081")
        
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1) # wait for server to start
    
    async with httpx.AsyncClient() as client:
        # Test Async
        res1 = await client.get('http://localhost:8081/test_async')
        print(res1.status_code, res1.headers, res1.json())
        assert res1.status_code == 200
        assert res1.json() == {"message": "async success"}
        assert res1.headers.get("x-process-time") == "0.123"
        
        # Test Sync
        res2 = await client.get('http://localhost:8081/test_sync')
        print(res2.status_code, res2.headers, res2.json())
        assert res2.status_code == 200
        assert res2.json() == {"message": "sync success"}
        assert res2.headers.get("x-process-time") == "0.123"
        
        # Test Route MW
        res3 = await client.get('http://localhost:8081/test_route_mw')
        assert res3.status_code == 200
        assert res3.headers.get("x-route-mw") == "route"
        assert res3.headers.get("x-process-time") == "0.123"

        # Test Router MW
        res4 = await client.get('http://localhost:8081/router/test')
        print("RES4:", res4.status_code, res4.headers, res4.content)
        assert res4.status_code == 200
        assert res4.headers.get("x-route-mw") == "route"
        assert res4.headers.get("x-router-mw") == "router"
        assert res4.headers.get("x-process-time") == "0.123"
        
        print("Middleware test passed!")

if __name__ == "__main__":
    asyncio.run(main())
