---
title: Multi-Protocol APIs
description: Serve REST, GraphQL, gRPC, and JSON-RPC from a single JustAPI application.
---

JustAPI supports four API protocols out of the box, all within a single application.

## 1. REST API (OpenAPI 3.1)

The default protocol. All route decorators generate OpenAPI 3.1 documentation automatically.

```python
@app.get("/items/{item_id}")
def read_item(request, item_id: int):
    return {"item_id": item_id}
```

**Interactive docs:**
| URL | UI |
|---|---|
| `/docs` | Swagger UI |
| `/redoc` | ReDoc |
| `/scalar` | Scalar API Reference |
| `/openapi.json` | Raw spec |

## 2. GraphQL

Mount a GraphQL endpoint with GraphiQL UI in one line:

```python
app.graphql(path="/graphql")
# GraphiQL IDE at http://localhost:8000/graphql
```

GraphQL uses `async-graphql` under the hood, compiled to Rust native code.

## 3. gRPC / Protobuf

Define a Protobuf service and mount it:

```python
from justapi import Schema


class RPCRequest(Schema):
    method: str
    params: dict


@app.post("/rpc", body_schema=RPCRequest)
def handle_rpc(request):
    data = request.json()
    return {"status": "OK", "method": data["method"]}
```

gRPC uses `tonic` and `prost` in the Rust runtime for high-performance binary serialization.

## 4. JSON-RPC 2.0

Standard JSON-RPC 2.0 endpoint:

```python
from justapi import Schema


class JSONRPCRequest(Schema):
    jsonrpc: str = "2.0"
    method: str
    params: list | dict = []
    id: int | str = 1


@app.post("/jsonrpc", body_schema=JSONRPCRequest)
def handle_jsonrpc(request):
    req = request.json()
    if req["method"] == "ping":
        return {"jsonrpc": "2.0", "result": "pong", "id": req["id"]}
```

## Protocol Comparison

| Feature | REST | GraphQL | gRPC | JSON-RPC |
|---|---|---|---|---|
| Data format | JSON | JSON (query) | Protobuf (binary) | JSON |
| Schema | OpenAPI | GraphQL Schema | `.proto` file | JSON Schema |
| Caching | Native HTTP | Manual | Manual | Manual |
| Streaming | SSE | Subscriptions | Bidirectional | No |
| Best for | General APIs | Complex queries | Internal services | RPC/Web3 |
| JustAPI setup | Auto | `app.graphql()` | `@app.post` | `@app.post` |

## Scaffolding with CLI

```bash
# Generate a project with your preferred protocol
justapi create my_api --api-type graphql
justapi create my_api --api-type grpc
justapi create my_api --api-type jsonrpc
justapi create my_api --api-type rest
```

## See Also

- [CLI Project Scaffolder](/getting-started/cli-scaffolder/) — Generate multi-protocol projects
- [JustAPIApp](/api-reference/justapiapp/) — `graphql()` method
- [Streaming Output](/advanced/streaming-output/) — SSE and streaming responses
