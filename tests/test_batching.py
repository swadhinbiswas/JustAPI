import asyncio
import time
from justapi.app import JustAPIApp, adaptive_batch
import urllib.request
import threading

app = JustAPIApp()

@app.post("/predict")
@adaptive_batch(max_size=3, window_ms=50)
def predict(requests):
    # Log the size of the batch
    print(f"Received batch of size {len(requests)}")
    # Simulate ML model computation
    time.sleep(0.1)
    
    results = []
    for req in requests:
        body = req.get("body", b"").decode("utf-8")
        results.append({"status": 200, "body": f"Processed {body}"})
    return results

def test_batching():
    def run_server():
        try:
            app.run("127.0.0.1:8081")
        except Exception:
            pass

    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()

    time.sleep(1)

    def make_request(idx):
        req = urllib.request.Request("http://127.0.0.1:8081/predict", data=f"req-{idx}".encode("utf-8"), method="POST")
        try:
            with urllib.request.urlopen(req) as response:
                return response.read().decode("utf-8")
        except Exception as e:
            return str(e)

    threads = []
    for i in range(5):
        t = threading.Thread(target=lambda i=i: print(f"Response {i}: {make_request(i)}"))
        threads.append(t)
        t.start()

    for t in threads:
        t.join()

    print("Done")
