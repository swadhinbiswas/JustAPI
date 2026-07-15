from justapi import JustAPIApp, Schema
import json

app = JustAPIApp()

@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))

# Variant B: native=False, WITH schema (non-native route w/ schema)
class ItemSchema(Schema):
    id: int
    name: str
    price: float

@app.post("/with_schema", schema=ItemSchema)
def with_schema(request):
    return json.loads(request["body"].decode("utf-8"))

if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8246"
    app.run(f"127.0.0.1:{port}")
