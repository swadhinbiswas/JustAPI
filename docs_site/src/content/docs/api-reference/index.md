---
title: API Reference Overview
description: "API reference overview for JustAPI, the FastAPI alternative — covers all classes, functions, and parameters."
keywords: [api reference, fastapi alternative, justapi, framework documentation]
---

The API Reference covers every public class, function, and configuration option in the JustAPI framework.

## Core Classes

| Class | Description |
|---|---|
| [JustAPIApp](/api-reference/justapiapp/) | Main application class. Register routes, middleware, and plugins. |
| [APIRouter](/api-reference/apirouter/) | Modular router for grouping related routes. |
| [Request](/api-reference/request/) | Incoming HTTP request with zero-copy access. |
| [Response](/api-reference/responses/) | Response classes for different content types. |
| [BackgroundTasks](/api-reference/background-tasks/) | Post-response background task execution. |
| [UploadFile](/api-reference/uploadfile/) | Uploaded file representation. |
| [Database](/api-reference/database/) | Connection pool and query interface. |
| [PyScheduler](/api-reference/scheduler/) | Cron-based periodic task scheduler. |
| [Session](/api-reference/session/) | Agent session state management. |
| [JustAPITestClient](/api-reference/testing-client/) | In-process HTTP test client. |

## Functions & Decorators

| Module | Description |
|---|---|
| [Routing](/api-reference/routing/) | `@app.get()`, `@app.post()`, `@app.put()`, `@app.patch()`, `@app.delete()` |
| [Dependency Injection](/api-reference/dependency-injection/) | `Depends()`, `Path()`, `Query()`, `Header()`, `Cookie()`, `Body()`, `File()`, `Form()` |
| [Exceptions](/api-reference/exceptions/) | `HTTPException`, `RequestValidationError` |
| [Schema & Validation](/api-reference/schema-validation/) | `Schema`, `Field()`, `validate_value()` |
| [WebSockets](/api-reference/websockets/) | `@app.websocket()`, `WebSocket` class |
| [Plugins](/api-reference/plugins/) | Plugin hooks: `build()`, `on_startup()`, `on_shutdown()` |

## Quick Links

- [Getting Started](/getting-started/overview/) — Quick introduction
- [Tutorials](/tutorials/hello-world/) — Step-by-step guides
- [Source Code](https://github.com/swadhinbiswas/JustAPI) — GitHub repository
