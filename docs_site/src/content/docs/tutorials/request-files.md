---
title: Request Files
description: Handle file uploads with UploadFile in JustAPI.
keywords: [JustAPI, file upload, UploadFile, multipart, binary]
---

## Basic File Upload

```python
from justapi import JustAPIApp, UploadFile, File

app = JustAPIApp()

@app.post("/upload")
async def upload_file(file: UploadFile = File(...)):
    content = await file.read()
    return {
        "filename": file.filename,
        "content_type": file.content_type,
        "size": len(content),
    }
```

## Multiple File Upload

```python
@app.post("/upload-multiple")
async def upload_multiple(files: list[UploadFile] = File(...)):
    results = []
    for file in files:
        content = await file.read()
        results.append({"filename": file.filename, "size": len(content)})
    return {"files": results}
```

## Save File to Disk

```python
import shutil
from pathlib import Path

UPLOAD_DIR = Path("uploads")
UPLOAD_DIR.mkdir(exist_ok=True)

@app.post("/upload-save")
async def upload_and_save(file: UploadFile = File(...)):
    file_path = UPLOAD_DIR / file.filename
    with open(file_path, "wb") as buffer:
        shutil.copyfileobj(file.file, buffer)
    return {"saved_to": str(file_path)}
```

## UploadFile Properties

| Property | Type | Description |
|----------|------|-------------|
| `filename` | `str` | Original filename |
| `content_type` | `str` | MIME type |
| `file` | `BinaryIO` | File-like object |
| `read()` | `await bytes` | Read entire file |
| `write()` | `await None` | Write data |

## See Also

- [Request Forms & Files](/tutorials/request-forms-files/) — forms with files
- [Form Data](/tutorials/form-data/) — form-only submissions
- [UploadFile Reference](/api-reference/uploadfile/) — full API reference
