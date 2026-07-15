from justapi import JustAPIApp
import json

app = JustAPIApp()


@app.post("/body_json")
def body_json(request):
    d = json.loads(request["body"].decode("utf-8"))
    return {"ok": True}


if __name__ == "__main__":
    app.run("127.0.0.1:8250")
