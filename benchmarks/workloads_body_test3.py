from justapi import JustAPIApp, Schema
import json

app = JustAPIApp()


@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))


class NativeItemSchema(Schema):
    id: int
    name: str
    price: float


@app.post("/validate_native", schema=NativeItemSchema, native=True)
def validate_native(request):
    raise AssertionError("handler must not run in native mode")


if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8244"
    app.run(f"127.0.0.1:{port}")
