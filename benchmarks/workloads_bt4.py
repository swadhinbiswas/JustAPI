from justapi import JustAPIApp, Schema
import json

app = JustAPIApp()

@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))

# Variant A: native=True, NO schema
@app.post("/native_ns", native=True)
def native_ns(request):
    return {"ok": True}

if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8245"
    app.run(f"127.0.0.1:{port}")
