import threading
import time
import urllib.request
import urllib.error
from justapi import JustAPIApp, Path, RequestValidationError, JSONResponse

app = JustAPIApp()

@app.get("/items/{item_id}")
def get_item(item_id: int = Path()):
    return {"item": item_id}

def validation_exception_handler(request, exc):
    return JSONResponse({"detail": exc.errors}, status_code=422)

app.add_exception_handler(RequestValidationError, validation_exception_handler)

def run_server():
    app.run("127.0.0.1:8084")

if __name__ == "__main__":
    t = threading.Thread(target=run_server, daemon=True)
    t.start()
    time.sleep(1)

    try:
        req = urllib.request.Request("http://127.0.0.1:8084/items/abc")
        with urllib.request.urlopen(req) as response:
            print("Success:", response.read().decode())
    except urllib.error.HTTPError as e:
        print(f"Error {e.code}: {e.read().decode()}")
