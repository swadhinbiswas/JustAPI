"""Robyn workload matching justapi's benchmark endpoints (hello + echo)."""

from robyn import Robyn

app = Robyn(__file__)


@app.get("/hello")
def hello(request):
    return {"message": "hello, world"}


@app.post("/echo")
def echo(request):
    return request.json()


if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8080"
    app.start(host="127.0.0.1", port=int(port))
