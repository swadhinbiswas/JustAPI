from justapi import JustAPIApp
import json

app = JustAPIApp()

def hello(request):
    return {"message": "hello, world"}

app.get("/hello", hello)

async def hello_async(request):
    import asyncio
    await asyncio.sleep(0.001)
    return {"message": "hello, world async"}

app.get("/hello_async", hello_async)
def echo(request):
    try:
        data = json.loads(request["body"].decode("utf-8"))
        return data
    except Exception:
        return {
            "status": 400, 
            "headers": [(b"content-type", b"application/json")],
            "body": b'{"error":"invalid JSON"}'
        }

app.post("/echo", echo)

if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8080"
    app.run(f"127.0.0.1:{port}")
