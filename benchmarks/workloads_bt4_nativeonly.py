from justapi import JustAPIApp
import json
app = JustAPIApp()
@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))
@app.post("/native_ns", native=True)
def native_ns(request):
    return {"ok": True}
if __name__ == "__main__":
    import sys
    app.run(f"127.0.0.1:{sys.argv[1]}")
