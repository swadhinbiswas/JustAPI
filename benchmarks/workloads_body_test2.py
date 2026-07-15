from justapi import JustAPIApp, Schema
import json

app = JustAPIApp()


@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))


@app.get("/hello")
def hello(request):
    return {"message": "hello, world"}


async def hello_async(request):
    import asyncio
    await asyncio.sleep(0.001)
    return {"message": "hello, world async"}


app.get("/hello_async", hello_async)


class NativeItemSchema(Schema):
    id: int
    name: str
    price: float


@app.post("/validate_native", schema=NativeItemSchema, native=True)
def validate_native(request):
    raise AssertionError("handler must not run in native mode")


if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8243"
    app.run(f"127.0.0.1:{port}")
