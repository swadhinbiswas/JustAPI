import urllib.request
from urllib.error import HTTPError
import multiprocessing
import time
import socket
from justapi import JustAPIApp

# Use dynamic port to avoid conflicts
def get_free_port():
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as s:
        s.bind(('', 0))
        return s.getsockname()[1]

PORT = get_free_port()

def run_server():
    app = JustAPIApp()
    app.enable_circuit_breaker(failure_threshold=2, reset_timeout_ms=5000)

    # Route that always fails
    @app.get("/flaky")
    def flaky(req):
        raise Exception("Random failure!")

    # Route that works
    @app.get("/ok")
    def ok(req):
        return {"status": "ok"}

    app.run(f"127.0.0.1:{PORT}")

def fetch(url):
    try:
        urllib.request.urlopen(url)
        return 200
    except HTTPError as e:
        return e.code
    except Exception as e:
        return 0

def test_circuit_breaker():
    p = multiprocessing.Process(target=run_server)
    p.start()
    # Wait for server to be ready with retry
    for _ in range(20):
        try:
            urllib.request.urlopen(f"http://127.0.0.1:{PORT}/ok", timeout=1)
            break
        except Exception:
            time.sleep(0.5)

    try:
        # Request 1: fails, should return 500 (Python exception caught and mapped to 500)
        status1 = fetch(f"http://127.0.0.1:{PORT}/flaky")
        assert status1 == 500, f"Expected 500, got {status1}"

        # Request 2: fails, should return 500
        status2 = fetch(f"http://127.0.0.1:{PORT}/flaky")
        assert status2 == 500, f"Expected 500, got {status2}"

        # Request 3: Circuit is now OPEN! Should return 503
        status3 = fetch(f"http://127.0.0.1:{PORT}/flaky")
        assert status3 == 503, f"Expected 503 Circuit Open, got {status3}"

        # Make sure another route is unaffected
        status_ok = fetch(f"http://127.0.0.1:{PORT}/ok")
        assert status_ok == 200, f"Expected 200, got {status_ok}"

        print("Circuit breaker tested successfully!")
    finally:
        p.terminate()
        p.join(timeout=5)

if __name__ == "__main__":
    test_circuit_breaker()
