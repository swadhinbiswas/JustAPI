"""Integration tests for the HTTP QUERY method (RFC 10008)."""

import multiprocessing
import time

import pytest
import requests

from justapi import JustAPIApp

PORT = 8095


def run_server():
    app = JustAPIApp()

    @app.query("/search")
    def search(request):
        return {"query": request.get("body", b"").decode("utf-8")}

    @app.get("/hello")
    def hello(req):
        return {"message": "hello"}

    app.run(f"127.0.0.1:{PORT}")


@pytest.fixture(scope="module")
def server():
    p = multiprocessing.Process(target=run_server)
    p.start()
    time.sleep(1.5)
    try:
        yield
    finally:
        p.terminate()
        p.join()


def test_query_returns_body(server):
    r = requests.request(
        "QUERY",
        f"http://127.0.0.1:{PORT}/search",
        data="q=hello&limit=10",
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    assert r.status_code == 200
    assert r.json() == {"query": "q=hello&limit=10"}


def test_query_requires_content_type(server):
    # Suppress the default Content-Type that `requests` would add for `data`.
    r = requests.request(
        "QUERY",
        f"http://127.0.0.1:{PORT}/search",
        data="q=hello",
        headers={"Content-Type": None},
    )
    assert r.status_code == 400
    assert "Content-Type" in r.text


def test_unknown_query_path_is_404(server):
    r = requests.request(
        "QUERY",
        f"http://127.0.0.1:{PORT}/missing",
        data="q=hello",
        headers={"Content-Type": "application/x-www-form-urlencoded"},
    )
    assert r.status_code == 404


def test_get_still_works(server):
    r = requests.get(f"http://127.0.0.1:{PORT}/hello")
    assert r.status_code == 200
    assert r.json() == {"message": "hello"}
