from justapi import JustAPIApp
import json

app = JustAPIApp()


@app.post("/noop")
def noop(request):
    return {"ok": True}


@app.post("/body_access")
def body_access(request):
    b = request["body"]
    return {"ok": True}


@app.post("/body_decode")
def body_decode(request):
    b = request["body"].decode("utf-8")
    return {"ok": True}


@app.post("/body_json")
def body_json(request):
    d = json.loads(request["body"].decode("utf-8"))
    return {"ok": True}


@app.post("/body_ret")
def body_ret(request):
    return request["body"]


if __name__ == "__main__":
    app.run("127.0.0.1:8242")
