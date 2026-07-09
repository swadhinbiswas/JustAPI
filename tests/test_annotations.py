import threading
import time
import urllib.request
from justapi import JustAPIApp, Path, Query, Header, JSONResponse

app = JustAPIApp()

@app.get("/hello/{name}")
def hello(name: str = Path(), count: int = Query(1), x_token: str = Header(alias="X-Token", default="abc")):
    return JSONResponse({
        "name": name,
        "count": count,
        "token": x_token,
    })

def run_server():
    app.run("127.0.0.1:8083")

if __name__ == "__main__":
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1)

    req = urllib.request.Request("http://127.0.0.1:8083/hello/world?count=5", headers={"X-Token": "xyz"})
    with urllib.request.urlopen(req) as response:
        print(response.read().decode())
