---
title: UploadFile API
description: "API reference for UploadFile in JustAPI, the FastAPI alternative — representation of uploaded files via multipart/form-data."
keywords: [uploadfile, fastapi alternative, justapi, file upload, multipart form data, upload]
---

## `UploadFile` Object

Represents a file uploaded via `multipart/form-data`. Files are streamed to temporary files by the Rust `multer` parser.

```python
from justapi import UploadFile, File
```

### Properties

| Attribute | Type | Description |
|---|---|---|
| `filename` | `str` | Original filename from the client |
| `content_type` | `str` | MIME type (e.g., `image/png`) |
| `file` | `str` | Path to the temporary file on disk |
| `size` | `int` | File size in bytes |

### Methods

| Method | Returns | Description |
|---|---|---|
| `.read()` | `bytes` | Read file contents into memory |
| `.write(data)` | `int` | Write bytes to the temp file |
| `.seek(pos)` | `None` | Seek to position in the temp file |
| `.close()` | `None` | Close and clean up the temp file |

### Usage

```python
@app.post("/upload")
async def upload(request, file: UploadFile = File(...)):
    content = await file.read()
    return {
        "name": file.filename,
        "type": file.content_type,
        "size": file.size,
        "content": content.decode(),
    }
```

## `File()`

Declare a file parameter:

```python
File(
    default=...,       # Use ... for required
    description=None,  # OpenAPI description
)
```

## `Form()`

Declare a form field parameter:

```python
Form(
    default=...,
    description=None,
)
```

## See Also

- [File Uploads Tutorial](/tutorials/file-uploads/) — Usage guide
- [Dependency Injection API](/api-reference/dependency-injection/) — File and Form extractors
