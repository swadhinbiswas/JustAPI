"""End-to-end tests for request coalescing (singleflight).

These exercise the Python ``enable_request_coalescing`` API through the
in-process test client, which applies the same Rust middleware chain as the
real server.
"""

import asyncio
import threading
import time

import pytest
import pytest_asyncio

from justapi import JustAPIApp
from justapi.testing import AsyncTestClient, assert_ok, assert_json


@pytest_asyncio.fixture
async def coalesced_app():
    app = JustAPIApp()
    app.enable_request_coalescing()

    state = {"calls": 0}
    lock = threading.Lock()

    def handler(_request):
        with lock:
            state["calls"] += 1
        # Simulate an expensive read so concurrent requests overlap in time,
        # making coalescing observable.
        time.sleep(0.2)
        return {"hello": "world"}

    app.get("/hot", handler)
    app._state = state
    return app


@pytest.mark.asyncio
async def test_concurrent_requests_coalesce(coalesced_app):
    async with AsyncTestClient(coalesced_app) as client:
        results = await asyncio.gather(*[client.get("/hot") for _ in range(10)])

    for resp in results:
        assert_ok(resp)
        assert_json(resp, {"hello": "world"})

    # Despite ten concurrent requests, the handler ran exactly once.
    assert coalesced_app._state["calls"] == 1


@pytest.mark.asyncio
async def test_distinct_paths_not_coalesced(coalesced_app):
    async with AsyncTestClient(coalesced_app) as client:
        a, b = await asyncio.gather(client.get("/hot"), client.get("/missing"))
    assert a["status"] == 200
    assert b["status"] == 404
    assert coalesced_app._state["calls"] == 1
