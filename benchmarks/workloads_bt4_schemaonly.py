from justapi import JustAPIApp, Schema
import json
app = JustAPIApp()
@app.post("/body_json")
def body_json(request):
    return json.loads(request["body"].decode("utf-8"))
if __name__ == "__main__":
    import sys
    app.run(f"127.0.0.1:{sys.argv[1]}")
