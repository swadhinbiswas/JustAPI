"""Benchmark workload applications for baseline measurements.

These are minimal ASGI applications used to benchmark Uvicorn, Granian,
and Hypercorn. JustAPI will eventually serve the same workloads for
apples-to-apples comparison.

Usage:
    uvicorn benchmarks.workloads:app --host 127.0.0.1 --port 8080 --workers 4
    granian --interface asgi benchmarks.workloads:app --host 127.0.0.1 --port 8080 --workers 4
"""

import json


async def app(scope, receive, send):
    """Minimal ASGI application with hello-world and JSON-echo routes."""
    if scope["type"] == "lifespan":
        while True:
            message = await receive()
            if message["type"] == "lifespan.startup":
                await send({"type": "lifespan.startup.complete"})
            elif message["type"] == "lifespan.shutdown":
                await send({"type": "lifespan.shutdown.complete"})
                return
        return

    if scope["type"] != "http":
        return

    path = scope["path"]
    method = scope["method"]

    if path == "/hello" and method == "GET":
        body = json.dumps({"message": "hello, world"}).encode("utf-8")
        await send({
            "type": "http.response.start",
            "status": 200,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(body)).encode()],
            ],
        })
        await send({"type": "http.response.body", "body": body})

    elif path == "/echo" and method == "POST":
        # Read the full request body
        body_parts = []
        while True:
            message = await receive()
            body_parts.append(message.get("body", b""))
            if not message.get("more_body", False):
                break
        request_body = b"".join(body_parts)

        # Parse and re-serialize (to exercise JSON round-trip)
        try:
            data = json.loads(request_body)
            response_body = json.dumps(data).encode("utf-8")
            status = 200
        except (json.JSONDecodeError, UnicodeDecodeError):
            response_body = b'{"error":"invalid JSON"}'
            status = 400

        await send({
            "type": "http.response.start",
            "status": status,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(response_body)).encode()],
            ],
        })
        await send({"type": "http.response.body", "body": response_body})

    else:
        body = b'{"error":"not found"}'
        await send({
            "type": "http.response.start",
            "status": 404,
            "headers": [
                [b"content-type", b"application/json"],
                [b"content-length", str(len(body)).encode()],
            ],
        })
        await send({"type": "http.response.body", "body": body})
