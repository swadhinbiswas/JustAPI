import threading
import time
import urllib.request
from justapi import JustAPIApp, JSONResponse, HTMLResponse, RedirectResponse, PlainTextResponse

app = JustAPIApp()

@app.get("/json")
def get_json():
    return JSONResponse({"hello": "world"}, status_code=201)

@app.get("/html")
def get_html():
    return HTMLResponse("<h1>Hello</h1>")

@app.get("/redirect")
def get_redirect():
    return RedirectResponse("/html", status_code=302)

@app.get("/plain")
def get_plain():
    return PlainTextResponse("Hello, world!")

def run_server():
    app.run("127.0.0.1:8082")

if __name__ == "__main__":
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1)

    # JSON
    req = urllib.request.urlopen("http://127.0.0.1:8082/json")
    print("JSON:", req.read().decode(), "Status:", req.status, "Headers:", req.headers.get("content-type"))
    
    # HTML
    req = urllib.request.urlopen("http://127.0.0.1:8082/html")
    print("HTML:", req.read().decode(), "Status:", req.status, "Headers:", req.headers.get("content-type"))
    
    # Plain
    req = urllib.request.urlopen("http://127.0.0.1:8082/plain")
    print("Plain:", req.read().decode(), "Status:", req.status, "Headers:", req.headers.get("content-type"))
    
    # Redirect
    try:
        req = urllib.request.urlopen("http://127.0.0.1:8082/redirect")
        print("Redirect reached:", req.read().decode(), req.status)
    except Exception as e:
        print("Redirect error:", e)
