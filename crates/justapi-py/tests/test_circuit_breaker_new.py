import urllib.request
from urllib.error import HTTPError
import multiprocessing
import time
from justapi import JustAPIApp

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

    app.run("127.0.0.1:8088")

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
    time.sleep(1.5)  # Wait for server to start

    try:
        # Request 1: fails, should return 500 (Python exception caught and mapped to 500)
        status1 = fetch("http://127.0.0.1:8088/flaky")
        assert status1 == 500, f"Expected 500, got {status1}"

        # Request 2: fails, should return 500
        status2 = fetch("http://127.0.0.1:8088/flaky")
        assert status2 == 500, f"Expected 500, got {status2}"

        # Request 3: Circuit is now OPEN! Should return 503
        status3 = fetch("http://127.0.0.1:8088/flaky")
        assert status3 == 503, f"Expected 503 Circuit Open, got {status3}"

        # Make sure another route is unaffected
        status_ok = fetch("http://127.0.0.1:8088/ok")
        assert status_ok == 200, f"Expected 200, got {status_ok}"

        print("Circuit breaker tested successfully!")
    finally:
        p.terminate()
        p.join()

if __name__ == "__main__":
    test_circuit_breaker()
