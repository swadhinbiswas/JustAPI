---
title: Help & Support
description: Where to get help with JustAPI — GitHub Issues, community support, and troubleshooting.
keywords: [JustAPI, help, support, GitHub, community, troubleshooting]
---

## GitHub Issues

For bug reports and feature requests:

**[GitHub Issues](https://github.com/swadhinbiswas/JustAPI/issues)**

When reporting a bug, include:
- Python version (`python --version`)
- JustAPI version (`python -c "import justapi; print(justapi.__version__)"`)
- Operating system
- Minimal code to reproduce the issue
- Full error traceback

## Stack Overflow

For how-to questions, tag your question with `justapi`:

**[Stack Overflow: justapi](https://stackoverflow.com/questions/tagged/justapi)**

## GitHub Discussions

For general questions, ideas, and community conversation:

**[GitHub Discussions](https://github.com/swadhinbiswas/JustAPI/discussions)**

## Security Vulnerabilities

Do **not** open a public GitHub issue for security vulnerabilities. Instead, follow the process in the [Security Policy](/security/policy/).

## Troubleshooting

### ImportError: No module named '_justapi'

The Rust extension wasn't built. Run `maturin develop --release`.

### `pip install` fails with build error

Ensure Rust is installed (`rustup --version`). Or use a pre-built wheel:

```bash
pip install --only-binary=justapi justapi
```

### Handler returns validation error for valid data

Check that your Pydantic model fields match the request body exactly. All fields without defaults are required.

### Port already in use

```bash
# Kill process on port 8000
lsof -ti:8000 | xargs kill -9
```

Or use a different port:

```python
app.run("127.0.0.1", 9000)
```
