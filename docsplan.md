# JustAPI Documentation Site — Full Rebuild Plan (FastAPI-Level Depth)

> **Goal:** Rebuild the docs_site (Astro + Starlight) to match FastAPI's documentation in depth, polish, navigation structure, and developer experience.
>
> **FastAPI docs analyzed:** https://fastapi.tiangolo.com/ — 7 top-level nav sections, ~140+ pages, auto-generated API reference, tutorial-first learning path, multilingual support, sponsor integration.
>
> **Current JustAPI docs_site:** ~20 pages across getting-started, tutorials, API reference, advanced, deployment, security, observability, contributing, inference, and reference sections. Content exists but lacks tutorial depth, API reference auto-generation, and structured learning path.

---

## 0. FastAPI Docs Analysis — What Makes It Great

### 0.1 Navigation Architecture (Top-Level)

| FastAPI Nav Item | Pages | Purpose |
|---|---|---|
| **Home** | 1 | Hero, sponsors, testimonials, quick-start, performance, dependencies |
| **Features** | 1 | Bullet-point feature showcase |
| **Learn** | ~76 | Main learning path — Python types intro, async intro, tutorial (35p), advanced guide (30p), CLI, editor support, deployment (8p), how-to recipes (11p) |
| **Reference** | ~21 | Auto-generated API reference — every class, function, parameter documented with signatures and examples |
| **Resources** | 7 | People, help, contributing, translations, project template, external links, newsletter |
| **About** | 4 | Alternatives/comparisons, history/design, benchmarks, repo management |
| **Release Notes** | 1 | Version changelog |

### 0.2 Key Design Patterns

1. **Tutorial-first learning path.** The `Learn` section is a structured course that builds from zero to advanced. Each page has:
   - Complete, runnable code example at the top
   - Step-by-step explanation
   - "Tip", "Note", "Info" callout boxes
   - Screenshots of Swagger UI / ReDoc
   - "Recap" section
   - Previous/Next page navigation
   - "On this page" right sidebar with section anchors
   - Breadcrumb navigation

2. **Python version tabs.** Code blocks are shown with Python 3.10+ by default, with tabs for older versions.

3. **Auto-generated API Reference.** The `/reference/` section is generated from docstrings — every parameter, return type, and method has a documented signature with examples.

4. **Sponsor integration.** Banner sponsors appear at the top of every page; sidebar sponsors on the home page.

5. **Translation support.** 12 languages with automatic language switcher in the top bar.

6. **"How To" recipes section.** 11 standalone recipes for specific tasks (GraphQL, custom middleware, database testing, etc.)

7. **Deployment depth.** 8 pages covering HTTPS, manual, cloud providers, Docker, server workers, concepts.

### 0.3 Page Count Breakdown

| Section | Subsection | Pages |
|---|---|---|
| Learn | Python Types Intro, Async/Await | 2 |
| Learn | Tutorial - User Guide | 35 (5 deps sub + 4 security sub) |
| Learn | Advanced User Guide | 30 (2 security sub) |
| Learn | FastAPI CLI | 1 |
| Learn | Editor Support | 1 |
| Learn | Deployment | 8 |
| Learn | How-To Recipes | 11 |
| Reference | API Reference | 21 (2 OpenAPI sub) |
| Resources | People, Help, Contributing, etc. | 7 |
| About | Alternatives, History, Benchmarks, Management | 4 |
| **Total** | | **~140+ pages** |

---

## 1. JustAPI Current State Audit

### 1.1 Existing Docs Inventory

| Section | Existing Pages | Status |
|---|---|---|
| Getting Started | overview, installation, first-steps, cli-scaffolder, migrating-from-fastapi | 5 pages exist, need enhancement |
| Tutorials | hello-world, path-query-params, request-body, dependency-injection, middleware, error-handling, file-uploads, background-tasks, websockets-sse, routing-subrouters, database-integration | 11 pages exist |
| API Reference | index, justapiapp, routing, apirouter, request, responses, dependency-injection, exceptions, schema-validation, websockets, background-tasks, scheduler, session, plugins, testing-client, uploadfile, database | 17 pages exist |
| Advanced | native-fast-path, pyo3-ffi-safety, rust-core-deep-dive, zero-gil-architecture, streaming-output, agent-system, multi-protocol-apis, performance-tuning, resilience-patterns | 9 pages exist |
| Deployment | production-checklist, docker, kubernetes-helm, cloudflare-pages, railway, flyio, aks, eks, gke | 9 pages exist |
| Security | secure-configuration, penetration-testing, owasp-compliance, policy | 4 pages exist |
| Observability | opentelemetry, health-checks, metrics-monitoring, structured-logging | 4 pages exist |
| Contributing | development-setup, coding-standards, testing-guide, benchmarking-guide, documentation-guide | 5 pages exist |
| Inference | overview, gpu-cuda-setup, llm-serving-api, scheduling-batching | 4 pages exist |
| Reference | cli, configuration, error-codes, adr-index, glossary, release-notes | 6 pages exist |
| Examples | index | 1 page exists |
| **Total** | | **~75 pages** |

### 1.2 Gaps vs FastAPI

| What FastAPI has | JustAPI status | Action |
|---|---|---|
| Tutorial with 35 progressive lessons | 11 tutorials, not fully progressive | Expand to 35+ tutorial pages |
| Advanced User Guide with 30 pages | 9 advanced pages | Add 21 advanced topics |
| How-To Recipes (11 pages) | 0 | Create new section |
| Auto-generated API Reference | 17 hand-written pages | Switch to docstring-generated |
| Python version tabs in code | Not supported | Add Starlight tab component |
| Multilingual support (12 langs) | Not supported | Add i18n routing |
| Sponsor banners on every page | Not supported | Add sponsor integration |
| "On this page" TOC with anchors | Starlight built-in | Already works |
| Breadcrumb navigation | Starlight built-in | Already works |
| Previous/Next page links | Starlight built-in | Already works |
| Search (Pagefind) | Starlight built-in | Already works |
| Screenshots in tutorials | 0 screenshots | Add screenshots |
| "Tip/Note/Warning" callout boxes | Starlight has `:::tip` etc. | Use consistently |
| Recap section in tutorials | Not used | Add to every tutorial |
| Editor support page | 0 | Create |
| FastAPI CLI reference | Has CLI reference | Enhance |
| Deployment cloud providers in docs | 9 deployment pages | Enhance |
| External links/resources page | 0 | Create |
| Project generation template page | 0 | Create |
| History, design and future | 0 | Create (DECISIONS.md) |
| Performance benchmarks page | BENCHMARKS.md exists | Port to docs site |
| Release notes changelog | release-notes.md exists | Enhance |

---

## 2. Rebuild Plan — Phases

### Phase 1: Information Architecture & Navigation Overhaul

**Goal:** Restructure sidebar to match FastAPI's learning-path-first approach.

#### 1.1 New Sidebar Structure

```
Getting Started (5 pages)
├── Overview & Philosophy
├── Installation
├── First Steps
├── CLI Project Scaffolder
└── Migrating from FastAPI

Tutorial — User Guide (35+ pages, progressive)
├── Python Types Intro (NEW)
├── Async / Await (NEW)
├── First Steps (existing, enhance)
├── Path Parameters (existing, enhance)
├── Query Parameters (existing, enhance)
├── Request Body (Pydantic) (existing, enhance)
├── Query Parameters & String Validations (NEW)
├── Path Parameters & Numeric Validations (NEW)
├── Query Parameter Models (NEW)
├── Body — Multiple Parameters (NEW)
├── Body — Fields (NEW)
├── Body — Nested Models (NEW)
├── Declare Request Example Data (NEW)
├── Extra Data Types (NEW)
├── Cookie Parameters (NEW)
├── Header Parameters (NEW)
├── Cookie Parameter Models (NEW)
├── Header Parameter Models (NEW)
├── Response Model — Return Type (NEW)
├── Extra Models (NEW)
├── Response Status Code (NEW)
├── Form Data (NEW)
├── Form Models (NEW)
├── Request Files (NEW)
├── Request Forms & Files (NEW)
├── Handling Errors (existing, enhance)
├── Path Operation Configuration (NEW)
├── JSON Compatible Encoder (NEW)
├── Body — Updates (NEW)
├── Dependencies (existing, expand)
│   ├── Classes as Dependencies (NEW)
│   ├── Sub-dependencies (NEW)
│   ├── Dependencies in Path Ops (NEW)
│   ├── Global Dependencies (NEW)
│   └── Dependencies with yield (NEW)
├── Security (expand)
│   ├── Security First Steps (NEW)
│   ├── Get Current User (NEW)
│   ├── Simple OAuth2 (NEW)
│   └── OAuth2 + JWT Tokens (NEW)
├── Middleware (existing, enhance)
├── CORS (existing)
├── SQL Databases (existing)
├── Bigger Applications — Multiple Files (existing)
├── Background Tasks (existing, enhance)
├── Metadata & Docs URLs (NEW)
├── Frontend (NEW)
├── Static Files (NEW)
├── Testing (existing, enhance)
└── Debugging (NEW)

Advanced User Guide (30 pages)
├── Native Fast Path (existing, enhance)
├── Zero-GIL Architecture (existing, enhance)
├── Rust Core Deep Dive (existing, enhance)
├── PyO3 FFI Safety (existing, enhance)
├── Streaming Output (existing, enhance)
├── Agent System (existing, enhance)
├── Multi-Protocol APIs (existing, enhance)
├── Performance Tuning (existing, enhance)
├── Resilience Patterns (existing, enhance)
├── Path Operation Advanced Configuration (NEW)
├── Additional Status Codes (NEW)
├── Return a Response Directly (NEW)
├── Custom Response — HTML, Stream, File (NEW)
├── Additional Responses in OpenAPI (NEW)
├── Response Cookies (NEW)
├── Response Headers (NEW)
├── Response — Change Status Code (NEW)
├── Advanced Dependencies (NEW)
├── Advanced Security (NEW)
│   ├── OAuth2 Scopes (NEW)
│   └── HTTP Basic Auth (NEW)
├── Using the Request Directly (NEW)
├── Using Dataclasses (NEW)
├── Advanced Middleware (NEW)
├── Sub Applications — Mounts (NEW)
├── Behind a Proxy (NEW)
├── Templates (Jinja2) (NEW)
├── WebSockets (existing, enhance)
├── Lifespan Events (NEW)
├── Testing WebSockets (NEW)
├── Testing Events (NEW)
├── Testing Dependencies with Overrides (NEW)
├── Async Tests (NEW)
├── Settings & Environment Variables (NEW)
├── OpenAPI Callbacks (NEW)
├── OpenAPI Webhooks (NEW)
├── Including WSGI — Flask, Django (NEW)
├── Generating SDKs (NEW)
└── Strict Content-Type Checking (NEW)

How-To Recipes (15 pages, NEW section)
├── General Recipes (NEW)
├── Migrate from Pydantic v1 (NEW)
├── GraphQL Integration (NEW)
├── Custom Request & Route Classes (NEW)
├── Conditional OpenAPI (NEW)
├── Extending OpenAPI (NEW)
├── Separate Input/Output Schemas (NEW)
├── Custom Docs UI Assets (NEW)
├── Configure Swagger UI (NEW)
├── Testing a Database (NEW)
├── Custom Error Status Codes (NEW)
├── gRPC Advanced Patterns (NEW)
├── WASM Middleware Plugins (NEW)
├── Circuit Breaker Recipes (NEW)
└── Background Task Patterns (NEW)

API Reference (20+ pages, auto-generated)
├── JustAPIApp — Constructor & Methods (enhance)
├── Request Parameters (Path, Query, Header, Cookie) (enhance)
├── Status Codes (enhance)
├── UploadFile (enhance)
├── Exceptions — HTTPException (enhance)
├── Dependencies — Depends() (enhance)
├── APIRouter (enhance)
├── BackgroundTasks (enhance)
├── Request (enhance)
├── WebSockets (enhance)
├── Response (enhance)
├── Custom Response Classes (enhance)
├── Middleware (enhance)
├── OpenAPI — Docs & Models (enhance)
├── Security Tools (enhance)
├── Encoders (enhance)
├── Static Files (enhance)
├── Templating — Jinja2Templates (enhance)
├── TestClient (enhance)
├── Schema — Validator & Fields (enhance)
└── Plugin System — Lifecycle Hooks (enhance)

Deployment (10 pages, enhance existing)
├── Production Checklist (enhance)
├── About HTTPS (NEW)
├── Run a Server Manually (NEW)
├── Deployment Concepts (NEW)
├── Docker (enhance)
├── Kubernetes / Helm (enhance)
├── Server Workers — Workers & Concurrency (NEW)
├── Google Cloud (GKE) (enhance)
├── Amazon (EKS) (enhance)
├── Azure (AKS) (enhance)
├── Fly.io (enhance)
├── Railway (enhance)
└── Cloudflare Pages (enhance)

Security (5 pages)
├── Security Policy (enhance)
├── OWASP Compliance (enhance)
├── Penetration Testing (enhance)
├── Secure Configuration (enhance)
└── Authentication & Authorization Guide (NEW)

Observability (5 pages)
├── OpenTelemetry Tracing (enhance)
├── Prometheus Metrics (enhance)
├── Structured Logging (enhance)
├── Health Checks (enhance)
└── Alerting & Incident Response (NEW)

Contributing (6 pages)
├── Development Setup (enhance)
├── Coding Standards (enhance)
├── Testing Guide (enhance)
├── Benchmarking Guide (enhance)
├── Documentation Guide (enhance)
└── Release Process (NEW)

Inference / AI (6 pages, existing)
├── Overview (enhance)
├── GPU / CUDA Setup (enhance)
├── LLM Serving API (enhance)
├── Scheduling & Batching (enhance)
├── Quantization & LoRA (NEW)
└── OpenAI-Compatible API (NEW)

Reference (8 pages)
├── CLI Reference (enhance)
├── Configuration Reference (enhance)
├── Error Codes (enhance)
├── ADR Index (enhance)
├── Glossary (enhance)
├── Release Notes (enhance)
├── Changelog (NEW)
└── Benchmarks (NEW, port from BENCHMARKS.md)

Resources (4 pages, NEW section)
├── Help & Support (NEW)
├── External Links & Ecosystem (NEW)
├── Full Stack Project Template (NEW)
└── Newsletter / Community (NEW)

About (4 pages, NEW section)
├── Alternatives, Inspiration & Comparisons (NEW)
├── History, Design & Future (NEW)
├── Benchmarks & Performance (NEW)
├── Repository Management (NEW)
└── License (NEW)
```

**Total planned pages: ~180+** (vs ~75 current, vs ~140 FastAPI)

---

### Phase 2: Tutorial Deep-Dive — Expand to 35+ Progressive Pages

**Pattern for each tutorial page (mirror FastAPI):**

```
---
title: <Topic>
description: <SEO description>
keywords: [<tags>]
---

# <Title>

<2-3 sentence introduction>

<Complete, runnable code example at top>

## <Step 1>

<Explanation with code snippets>

:::tip
<Tip callout>
:::

:::note
<Note callout>
:::

## <Step N>

<Screenshots of Swagger UI / ReDoc where applicable>

## Recap

<Summary of what was learned>

## See Also

- [Next Topic](/tutorial/next-topic/)
- [Related API Reference](/reference/related/)
```

#### 2.1 New Tutorials to Write (24 missing)

These are the tutorials FastAPI has that JustAPI doesn't:

1. **Python Types Intro** — `str`, `int`, `float`, `bool`, `list`, `dict`, `Optional`, `Union`, `Literal`, `TypedDict`, `dataclass`. How JustAPI uses type hints.
2. **Async / Await** — When to use `async def` vs `def`, concurrency model, GIL implications.
3. **Query Parameters & String Validations** — `Query()` with `min_length`, `max_length`, `pattern`, `deprecated`.
4. **Path Parameters & Numeric Validations** — `Path()` with `ge`, `le`, `gt`, `lt`.
5. **Query Parameter Models** — Using Pydantic models for query params.
6. **Body — Multiple Parameters** — Mixing path, query, body, singular body values.
7. **Body — Fields** — `Field()` for Pydantic model fields.
8. **Body — Nested Models** — Sub-models, deeply nested JSON.
9. **Declare Request Example Data** — `example`, `examples` in schema.
10. **Extra Data Types** — `UUID`, `datetime`, `date`, `bytes`, `Decimal`, `Base64`.
11. **Cookie Parameters** — Reading cookies.
12. **Header Parameters** — Reading headers.
13. **Cookie Parameter Models** — Pydantic models for cookies.
14. **Header Parameter Models** — Pydantic models for headers.
15. **Response Model — Return Type** — `response_model`, filtering fields.
16. **Extra Models** — Multiple related models, inheritance.
17. **Response Status Code** — Setting custom status codes.
18. **Form Data** — `Form()` for non-JSON forms.
19. **Form Models** — Pydantic models for forms.
20. **Request Files** — `File()` and `UploadFile`.
21. **Request Forms & Files** — Combining forms and files.
22. **Path Operation Configuration** — `tags`, `summary`, `description`, `response_description`, `deprecated`, `include_in_schema`.
23. **JSON Compatible Encoder** — `jsonable_encoder`.
24. **Body — Updates** — PATCH vs PUT, partial updates.

#### 2.2 Tutorials to Expand (11 existing)

1. **Hello World** — Add Python version tabs, run with `uv` not just `python main.py`, add `curl` testing examples, add OpenAPI docs section.
2. **Path & Query Parameters** — Split into path params and query params separate pages. Add validation sections.
3. **Request Body** — Add nested models section, example data, Field usage.
4. **Dependency Injection** — Add sub-dependencies, classes as deps, global deps, yield deps.
5. **Middleware** — Add ASGI middleware, custom middleware, third-party middleware.
6. **Error Handling** — Add custom exception handlers, override default handlers.
7. **File Uploads** — Add form models, multiple file uploads.
8. **Background Tasks** — Add task dependencies, error handling.
9. **WebSockets & SSE** — Split into separate pages. Add testing WebSockets section.
10. **Routing & Subrouters** — Add path operation configuration, metadata.
11. **Database Integration** — Add async SQL, migrations, testing.

---

### Phase 3: Advanced User Guide — Expand to 30+ Pages

#### 3.1 Existing to Enhance (9 pages)

1. **Native Fast Path** — Add performance comparison graphs, when to use `native=True` vs when not to.
2. **Zero-GIL Architecture** — Update with free-threaded Python 3.13t/3.14t section.
3. **Rust Core Deep Dive** — Add codebase navigation guide, key structs and traits.
4. **PyO3 FFI Safety** — Update with latest PyO3 patterns, `Bound` API.
5. **Streaming Output** — Add `@app.stream_json`, async generator patterns.
6. **Agent System** — Add MCP tool exposition guide, session management.
7. **Multi-Protocol APIs** — Add JSON-RPC and gRPC examples side-by-side.
8. **Performance Tuning** — Add worker config, connection pooling, `justapi.conf` reference.
9. **Resilience Patterns** — Add circuit breaker configuration, bulkhead, retry policies.

#### 3.2 New Advanced Pages to Write (21 pages)

1. **Path Operation Advanced Configuration** — `openapi_extra`, `response_model_exclude_unset`, etc.
2. **Additional Status Codes** — Returning custom status codes alongside 200.
3. **Return a Response Directly** — `Response`, `JSONResponse`, `HTMLResponse`.
4. **Custom Response — HTML, Stream, File** — `StreamingResponse`, `FileResponse`, `RedirectResponse`.
5. **Additional Responses in OpenAPI** — Documenting error responses, multiple response schemas.
6. **Response Cookies** — Setting and deleting cookies in responses.
7. **Response Headers** — Custom response headers.
8. **Response — Change Status Code** — Default status code override.
9. **Advanced Dependencies** — Parameterized dependencies, async generator deps.
10. **Advanced Security** — OAuth2 scopes, HTTP Basic Auth, API keys.
11. **Using the Request Directly** — Accessing raw request body, headers.
12. **Using Dataclasses** — Python dataclasses as request models.
13. **Advanced Middleware** — BaseHTTPMiddleware, ASGI middleware, third-party middleware integration.
14. **Sub Applications — Mounts** — Mounting sub-applications at sub-paths.
15. **Behind a Proxy** — `root_path`, `X-Forwarded-*` headers, reverse proxy config.
16. **Templates (Jinja2)** — Server-side rendering with Jinja2.
17. **WebSockets Advanced** — WebSocket connection management, broadcasting.
18. **Lifespan Events** — `startup`/`shutdown`, lifespan context manager.
19. **Testing WebSockets** — `TestClient` WebSocket testing.
20. **Testing Events** — Testing lifespan events.
21. **Testing Dependencies with Overrides** — `app.dependency_overrides`, test dependency injection.
22. **Async Tests** — `async` test functions, `httpx.AsyncClient`.
23. **Settings & Environment Variables** — Pydantic Settings, `.env` files.
24. **OpenAPI Callbacks** — Webhook request schemas in OpenAPI.
25. **OpenAPI Webhooks** — Outbound webhook definitions.
26. **Including WSGI — Flask, Django** — Mounting WSGI apps.
27. **Generating SDKs** — OpenAPI-based client generation (`openapi-generator`).
28. **Strict Content-Type Checking** — CSRF prevention, Content-Type enforcement.

---

### Phase 4: How-To Recipes (15 pages, NEW)

A standalone section for specific, task-oriented recipes:

1. **General How-To** — Common patterns and workarounds.
2. **Migrate from Pydantic v1** — `Field` changes, `model_dump()` vs `dict()`, validators.
3. **GraphQL Integration** — Setting up Strawberry GraphQL alongside REST.
4. **Custom Request & Route Classes** — Extending `Request` and `APIRoute`.
5. **Conditional OpenAPI** — Disabling OpenAPI in production.
6. **Extending OpenAPI** — Adding custom OpenAPI fields.
7. **Separate Input/Output Schemas** — Benefits and configuration.
8. **Custom Docs UI Assets** — Self-hosting Swagger UI / ReDoc.
9. **Configure Swagger UI** — `swagger_ui_parameters`, themes, syntax highlighting.
10. **Testing a Database** — In-memory SQLite for tests, transaction rollback.
11. **Custom Error Status Codes** — 403 → 401 migration patterns.
12. **gRPC Advanced Patterns** — Streaming RPCs, interceptors, auth.
13. **WASM Middleware Plugins** — Writing and deploying wasmtime plugins.
14. **Circuit Breaker Recipes** — Configuring resilience patterns for specific scenarios.
15. **Background Task Patterns** — Prioritized tasks, task cancellation, progress reporting.

---

### Phase 5: API Reference — Auto-Generated from Docstrings

**Current state:** 17 hand-written markdown pages.
**Target:** Auto-generated API reference from Python docstrings + Rust docstrings.

#### 5.1 Tools & Implementation

- Use `pydoc`-style generation or a Starlight plugin that parses Python stubs
- Generate from actual `.pyi` type stubs in `crates/justapi-py/python/justapi/`
- Document every public class, method, function, and parameter
- Each reference page includes:
  - Signature with type annotations
  - Description from docstring
  - Parameter table (name, type, default, description)
  - Example usage
  - See also links

#### 5.2 Reference Pages to Create/Enhance

| # | Page | Source | Priority |
|---|---|---|---|
| 1 | JustAPIApp | `justapi/__init__.py` — FastAPI docstring style | P0 |
| 2 | Routing (`@app.get/post/etc`) | Route decorators | P0 |
| 3 | APIRouter | `justapi/routing.py` | P0 |
| 4 | Request | Request object properties & methods | P0 |
| 5 | Response | Response, JSONResponse, StreamingResponse | P0 |
| 6 | Dependencies | `Depends()`, `Path()`, `Query()`, `Header()`, `Cookie()` | P0 |
| 7 | Exceptions | `HTTPException`, `RequestValidationError` | P0 |
| 8 | WebSockets | WebSocket handler, `WebSocket` class | P0 |
| 9 | Background Tasks | `BackgroundTasks`, `add_task` | P0 |
| 10 | Schema & Validation | `Schema`, `Field`, `body_schema` | P0 |
| 11 | UploadFile | `UploadFile` properties & methods | P0 |
| 12 | Testing Client | `JustAPITestClient`, `get`/`post`/etc | P0 |
| 13 | Middleware | `add_middleware`, built-in middleware classes | P1 |
| 14 | Security Tools | JWT, OAuth2 utilities | P1 |
| 15 | OpenAPI | Docs UI configuration, `openapi.json` | P1 |
| 16 | Static Files | `Mount` static files | P1 |
| 17 | Templating | Jinja2Templates | P1 |
| 18 | Scheduler | `PyScheduler`, cron/interval/once | P1 |
| 19 | Plugins | Plugin lifecycle hooks | P1 |
| 20 | Session | Session properties, agent state | P1 |
| 21 | Status Codes | `status` module reference | P2 |

---

### Phase 6: Deployment — Expand to 12 Pages

**Current:** 9 deployment pages.

**Enhancements:**
1. **Production Checklist** — Add Redis config, database pooling, logging levels, monitoring setup.
2. **About HTTPS** — NEW. TLS termination, certbot, rustls, Let's Encrypt.
3. **Run a Server Manually** — NEW. `justapi serve` flags, `--workers`, `--port`, `--host`.
4. **Deployment Concepts** — NEW. Concurrency, workers, auto-scaling, resource limits.
5. **Docker** — Add `docker-compose` examples with Redis, PostgreSQL, Prometheus.
6. **Kubernetes / Helm** — Add auto-scaling config, ingress examples, resource requests/limits.
7. **Server Workers** — NEW. Uvicorn workers vs JustAPI workers, Gunicorn integration.
8. Cloud deployment guides — Port from `deploy/` to docs_site with screenshots and CLI steps.

---

### Phase 7: Security & Observability — Expand and Polish

**Security (5 pages):**
- Add new **Authentication & Authorization Guide** — OAuth2 flows, JWT, API keys, RBAC.
- Enhance existing pages with screenshots, config examples, and troubleshooting.

**Observability (5 pages):**
- Add new **Alerting & Incident Response** — Prometheus AlertManager, PagerDuty integration, SLIs/SLOs.
- Enhance existing with Grafana dashboard JSON examples.

---

### Phase 8: Inference / AI Section (6 pages)

**Current:** 4 pages.
**Add:**
- **Quantization & LoRA** — Model quantization, LoRA adapter serving.
- **OpenAI-Compatible API** — `/v1/chat/completions` endpoint, token streaming, function calling.

---

### Phase 9: Resources, About, and Polish Sections

**Resources (NEW — 4 pages):**
1. **Help & Support** — GitHub Issues, Discord, Stack Overflow, FAQ.
2. **External Links & Ecosystem** — Related projects, plugins, community tools.
3. **Full Stack Project Template** — Guide to using the `justapi create` full-stack template.
4. **Newsletter / Community** — RSS feed, mailing list, social links.

**About (NEW — 5 pages):**
1. **Alternatives, Inspiration & Comparisons** — Port and expand from README competitors section.
2. **History, Design & Future** — Why Rust, design decisions (port from DECISIONS.md).
3. **Benchmarks & Performance** — Port from BENCHMARKS.md into searchable, navigable format.
4. **Repository Management** — Maintainers, governance, release process.
5. **License** — MIT License text.

---

### Phase 10: Infrastructure & CI

#### 10.1 Auto-Generation Pipeline
- Create a script (`scripts/gen_api_ref.py`) that parses `crates/justapi-py/python/justapi/` `.pyi` stubs and generates Starlight markdown pages in `docs_site/src/content/docs/reference/`
- Run this script as part of the docs build (`astro build`)

#### 10.2 Deployment
- **Cloudflare Pages** (current target) — already configured
- Add **PR preview deployments** for docs changes
- Add **link checker** in CI for docs builds
- Add **spell checker** (`cspell`) for documentation

#### 10.3 CI Integration
- Add docs build to `wheels.yml` (or a separate `docs.yml`)
- Fail build on broken links
- Auto-deploy on merge to `main`/`master`

#### 10.4 SEO & Performance
- Sitemap generation (`@astrojs/sitemap`)
- OpenGraph + Twitter Card meta tags on every page
- Canonical URLs
- Pagefind full-text search
- RSS feed for release notes (`@astrojs/rss`)
- Lighthouse target: 100/100

---

## 3. Page Templates

### 3.1 Tutorial Page Template

```markdown
---
title: <Topic Title>
description: <2-line SEO description>
keywords: [JustAPI, <topic>, Python web framework, FastAPI alternative]
---

# <Topic Title>

<2-3 sentence introduction explaining what this page covers and why it matters.

## <First Concept>

<Clear explanation with code snippet>

```python
# Example code
```

:::tip
<Tip that helps the reader>
:::

## <Second Concept>

<Screenshot or diagram if applicable>

![Swagger UI showing the feature](/img/tutorial/<path>.png)

## Recap

<Bullet-point summary of what was learned>

## See Also

- <Next logical tutorial>
- <Related API reference>
```

### 3.2 Reference Page Template

```markdown
---
title: <Class/Function Name>
description: API reference for <class/function>
keywords: [JustAPI, <class/function>, API reference]
---

# `<Class/Function Name>`

<Description of what this does>

## Signature

```python
<def signature with types>
```

## Parameters

| Parameter | Type | Default | Description |
|-----------|------|---------|-------------|
| `<name>` | `<type>` | `<default>` | <description> |

## Example

```python
<runnable example>
```

## See Also

- <Related tutorial>
- <Related reference>
```

---

## 4. Implementation Order

| Phase | Priority | Pages | Effort | Depends On |
|---|---|---|---|---|
| Phase 1: IA & Nav | P0 | Sidebar config | 1 day | Nothing |
| Phase 2: Tutorial Expansion | P0 | +24 new tutorials, 11 enhanced | 3 weeks | Phase 1 |
| Phase 3: Advanced Guide | P1 | +21 new, 9 enhanced | 2 weeks | Phase 1 |
| Phase 5: API Reference | P0 | 21 pages auto-generated | 1 week | Phase 1 |
| Phase 4: How-To Recipes | P1 | 15 new pages | 1 week | Phase 1 |
| Phase 6: Deployment | P1 | +3 new, 9 enhanced | 3 days | Phase 1 |
| Phase 7: Security & O11y | P2 | +2 new, 8 enhanced | 2 days | Phase 1 |
| Phase 8: Inference | P2 | +2 new, 4 enhanced | 2 days | Phase 1 |
| Phase 9: Resources & About | P2 | 9 new pages | 2 days | Phase 1 |
| Phase 10: Infrastructure | P2 | CI, auto-gen, SEO | 3 days | Phase 5 |

**Total estimated effort: ~8-10 weeks for a single contributor.**

---

## 5. Success Criteria

- [ ] Sidebar structure mirrors FastAPI — learning-path-first with progressive tutorials
- [ ] Tutorial — User Guide has 35+ progressive lessons with code, screenshots, and recaps
- [ ] Advanced User Guide has 30+ pages covering every framework feature
- [ ] API Reference auto-generated from `.pyi` stubs with full parameter docs
- [ ] How-To Recipes has 15 standalone task guides
- [ ] Deployment covers all 5 cloud providers with screenshots
- [ ] Security & Observability include configuration examples and dashboard JSON
- [ ] Resources, About, and Reference sections complete the 180-page target
- [ ] All pages have SEO meta tags, OpenGraph, and breadcrumbs
- [ ] docs build passes CI with no broken links
- [ ] Lighthouse 100/100 on all categories
- [ ] Search (Pagefind) indexes all content with section-level granularity
