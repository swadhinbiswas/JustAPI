---
title: API Stability
description: JustAPI's commitment to backward compatibility and stability.
---

# JustAPI API Stability Guarantees

JustAPI's commitment to backward compatibility and stability.

---

## Table of Contents

1. [Versioning Policy](#1-versioning-policy)
2. [Stability Tiers](#2-stability-tiers)
3. [Breaking Changes](#3-breaking-changes)
4. [Deprecation Policy](#4-deprecation-policy)
5. [Migration Support](#5-migration-support)

---

## 1. Versioning Policy

JustAPI follows [Semantic Versioning](https://semver.org/) (SemVer):

- **Major version (X.0.0):** Breaking changes
- **Minor version (0.X.0):** New features, backward-compatible
- **Patch version (0.0.X):** Bug fixes, backward-compatible

### Current Version

JustAPI is at **v2.0.x** (pre-1.0 stability guarantees apply).

### Pre-1.0 Stability

Before v1.0.0, JustAPI may introduce breaking changes in minor versions. However:
- Breaking changes will be documented in CHANGELOG.md
- Migration guides will be provided
- Deprecation warnings will be emitted before removal

---

## 2. Stability Tiers

### Tier 1: Stable API

These APIs are guaranteed backward-compatible within major versions:

| Component | Status | Notes |
|-----------|--------|-------|
| `JustAPIApp` class | ✅ Stable | Core application class |
| Route decorators | ✅ Stable | `@app.get()`, `@app.post()`, etc. |
| Request/Response objects | ✅ Stable | Starlette-compatible |
| Dependency injection | ✅ Stable | `Depends()` pattern |
| Schema validation | ✅ Stable | `Schema` class |
| Error handling | ✅ Stable | `HTTPException` |
| WebSocket API | ✅ Stable | `@app.websocket()` |
| SSE streaming | ✅ Stable | `StreamingResponse` |
| OpenAPI generation | ✅ Stable | Auto-generated specs |

### Tier 2: Experimental APIs

These APIs may change before v1.0:

| Component | Status | Notes |
|-----------|--------|-------|
| gRPC support | ⚠️ Experimental | API may evolve |
| GraphQL support | ⚠️ Experimental | API may evolve |
| ML inference | ⚠️ Experimental | API may evolve |
| MCP tools | ⚠️ Experimental | API may evolve |
| DAG engine | ⚠️ Experimental | API may evolve |

### Tier 3: Internal APIs

These are not part of the public API and may change without notice:

| Component | Status | Notes |
|-----------|--------|-------|
| `justapi-core` internals | ❌ Internal | Rust crate, not Python API |
| `justapi-py` internals | ❌ Internal | PyO3 bindings |
| CLI internals | ❌ Internal | Command-line tool |

---

## 3. Breaking Changes

### What Counts as Breaking

A breaking change is any modification that requires users to change their code:

- Removing or renaming a public function/class
- Changing function signatures
- Changing return types
- Changing exception types
- Changing default behavior

### What Does NOT Count as Breaking

- Adding new optional parameters
- Adding new modules/classes
- Fixing bugs that change behavior to match documentation
- Performance improvements
- Security fixes

### Breaking Change Process

1. **Announce** in GitHub Issues/Discussions
2. **Document** in CHANGELOG.md with migration guide
3. **Deprecate** old API with warnings (1 minor version)
4. **Remove** in next major version

---

## 4. Deprecation Policy

### Deprecation Timeline

1. **v0.X.0:** Feature deprecated with warning
2. **v0.X+1.0:** Warning persists
3. **v0.X+2.0:** Feature removed

### Deprecation Warnings

Deprecated APIs emit `DeprecationWarning`:

```python
import warnings
warnings.warn(
    "old_function() is deprecated, use new_function() instead",
    DeprecationWarning,
    stacklevel=2
)
```

### Finding Deprecated APIs

```bash
# Run with deprecation warnings enabled
python -W default::DeprecationWarning app.py

# Or filter specific warnings
python -W "default::DeprecationWarning:justapi" app.py
```

---

## 5. Migration Support

### Migration Guides

JustAPI provides migration guides for common scenarios:

- [From FastAPI](migration-from-fastapi.md)
- [From Robyn](migration-guide.md#migration-from-robyn)
- [From Granian](migration-guide.md#migration-from-granian)

### Automated Migration Tools

```bash
# Check for deprecated APIs
justapi check --deprecated

# Generate migration suggestions
justapi migrate --from fastapi
```

### Migration Support Timeline

| Version | Support |
|---------|---------|
| v0.x → v0.x+1 | Migration guide provided |
| v0.x → v1.0 | Full migration tool + guide |
| v1.x → v2.x | LTS migration support |

---

## 6. Stability承诺

### Our Commitment

JustAPI commits to:

1. **No surprise breaking changes** in minor/patch versions
2. **30-day notice** before breaking changes
3. **Migration guides** for all breaking changes
4. **LTS versions** for production deployments
5. **Security patches** for all supported versions

### Supported Versions

| Version | Status | Support End |
|---------|--------|-------------|
| v2.0.x | Active | Until v3.0 release |
| v1.0.x | LTS | 2 years after v2.0 |
| v0.x | End of Life | — |

### Getting Help

- **Breaking change concerns:** Open GitHub Issue
- **Migration help:** See migration guides
- **Enterprise support:** Contact maintainers
