---
title: Request Forms and Files
description: Handle both form data and file uploads in a single request in JustAPI.
keywords: [JustAPI, form data, file upload, combined, multipart]
---

## Combined Form + File

```python
from justapi import JustAPIApp, UploadFile, File, Form

app = JustAPIApp()

@app.post("/items/")
async def create_item(
    name: str = Form(...),
    description: str = Form(""),
    image: UploadFile = File(None),
):
    result = {"name": name, "description": description}
    if image:
        content = await image.read()
        result["image_size"] = len(content)
    return result
```

## Multiple Files + Form Data

```python
@app.post("/upload/")
async def upload_with_metadata(
    title: str = Form(...),
    tags: list[str] = Form([]),
    files: list[UploadFile] = File(...),
):
    return {
        "title": title,
        "tags": tags,
        "file_count": len(files),
        "filenames": [f.filename for f in files],
    }
```

## See Also

- [Form Data](/tutorials/form-data/) — forms without files
- [Request Files](/tutorials/request-files/) — files without forms
- [Form Models](/tutorials/form-models/) — Pydantic models for forms
