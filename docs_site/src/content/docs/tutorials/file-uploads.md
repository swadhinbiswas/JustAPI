---
title: File Uploads
description: Handle file uploads with multipart form data using UploadFile and File annotations.
---

JustAPI provides robust support for file uploads via `multipart/form-data` using Rust's `multer` parser, with streaming to temporary files to avoid memory exhaustion.

## Basic File Upload

```python
from justapi import JustAPIApp, UploadFile, File

app = JustAPIApp()


@app.post("/upload/")
async def upload_file(request, file: UploadFile = File(...)):
    return {
        "filename": file.filename,
        "content_type": file.content_type,
        "size": file.size,
    }
```

```bash
curl -X POST http://127.0.0.1:8000/upload/ \
  -F "file=@example.txt"
# Output: {"filename":"example.txt","content_type":"text/plain","size":1234}
```

## Multiple Files

```python
@app.post("/upload-multiple/")
async def upload_multiple(request, files: list[UploadFile] = File(...)):
    return {
        "files": [
            {"name": f.filename, "size": f.size} for f in files
        ],
        "count": len(files),
    }
```

```bash
curl -X POST http://127.0.0.1:8000/upload-multiple/ \
  -F "files=@a.txt" \
  -F "files=@b.txt"
```

## File + Form Fields Together

```python
@app.post("/profile/")
async def create_profile(
    request,
    avatar: UploadFile = File(...),
    name: str = Form(...),
    bio: str = Form(""),
):
    return {
        "name": name,
        "bio": bio,
        "avatar_filename": avatar.filename,
        "avatar_size": avatar.size,
    }
```

```bash
curl -X POST http://127.0.0.1:8000/profile/ \
  -F "avatar=@photo.jpg" \
  -F "name=Alice" \
  -F "bio=Hello world"
```

## The `UploadFile` Object

| Attribute/Method | Type | Description |
|---|---|---|
| `filename` | str | Original filename from the client |
| `content_type` | str | MIME type of the file |
| `file` | str | Path to the temporary file on disk |
| `size` | int | File size in bytes |
| `.read()` | bytes | Read file contents into memory |

## How It Works

1. The Rust `multer` parser streams multipart data as it arrives
2. Each uploaded file is written to a temporary file on disk (not held in memory)
3. The `UploadFile` Python object provides a zero-copy view of the file path
4. Temporary files are cleaned up when the request completes

## Asynchronous Processing

For large files, process asynchronously to avoid blocking:

```python
import aiofiles
from justapi import UploadFile, File


@app.post("/process-large/")
async def process_large(request, file: UploadFile = File(...)):
    # Read and process in chunks
    content = await file.read()
    result = await process_content(content)
    return {"result": result}
```

## Next Steps

- [WebSockets & SSE](/tutorials/websockets-sse/) — Real-time bidirectional communication
- [Database Integration](/tutorials/database-integration/) — Store file metadata in DB
- [Background Tasks](/tutorials/background-tasks/) — Process files after the response
