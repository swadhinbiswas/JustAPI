import requests
import threading
import time

from justapi import JustAPIApp


def _spawn(addr, **kwargs):
    app = JustAPIApp()

    @app.post("/echo")
    def echo(request):
        return request.body

    def run():
        app.run(addr, **kwargs)

    t = threading.Thread(target=run, daemon=True)
    t.start()
    time.sleep(1.0)
    return app


def test_max_body_size_enforced():
    addr = "127.0.0.1:9231"
    _spawn(addr, max_body_size=1024)

    # Body within the limit is accepted.
    ok = requests.post(f"http://{addr}/echo", data=b"x" * 512)
    assert ok.status_code == 200, ok.status_code

    # Body over the limit is rejected with 413.
    big = requests.post(f"http://{addr}/echo", data=b"x" * (1024 + 1))
    assert big.status_code == 413, big.status_code
    assert b"payload too large" in big.content


def test_max_body_size_default_accepts_large():
    addr = "127.0.0.1:9232"
    _spawn(addr)

    # Default limit is 50 MiB; a 5 MiB body should pass.
    ok = requests.post(f"http://{addr}/echo", data=b"x" * (5 * 1024 * 1024))
    assert ok.status_code == 200, ok.status_code


if __name__ == "__main__":
    test_max_body_size_enforced()
    test_max_body_size_default_accepts_large()
    print("ok")
