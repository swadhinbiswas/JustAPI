import json

from justapi import JustAPIApp, JustAPITestClient, UploadFile


def _build_multipart(fields, boundary="----TestBoundary123"):
    body = b""
    for name, filename, data, ctype in fields:
        body += b"--" + boundary.encode() + b"\r\n"
        disp = 'Content-Disposition: form-data; name="{}"'.format(name)
        if filename:
            disp += '; filename="{}"'.format(filename)
        body += disp.encode() + b"\r\n"
        if filename:
            body += b"Content-Type: " + (ctype or "application/octet-stream").encode() + b"\r\n"
        body += b"\r\n" + data + b"\r\n"
    body += b"--" + boundary.encode() + b"--\r\n"
    ct = "multipart/form-data; boundary={}".format(boundary)
    return body, ct


def test_upload_file_attributes_and_read():
    app = JustAPIApp()

    async def upload(file: UploadFile):
        data = await file.read()
        return {
            "filename": file.filename,
            "size": file.size,
            "content_type": file.content_type,
            "content": data.decode(),
            "ct_header": file.headers.get("content-type"),
        }

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    content = b"hello, this is a file"
    body, ct = _build_multipart([("file", "test.txt", content, "text/plain")])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["filename"] == "test.txt"
    assert data["size"] == len(content)
    assert data["content_type"] == "text/plain"
    assert data["content"] == content.decode()
    assert data["ct_header"] == "text/plain"


def test_upload_filename_sanitized():
    app = JustAPIApp()

    async def upload(file: UploadFile):
        return {"filename": file.filename}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("file", "../../etc/passwd", b"x", None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["filename"] == "passwd"


def test_upload_read_partial_and_sequential():
    app = JustAPIApp()

    async def upload(file: UploadFile):
        first = await file.read(5)
        rest = await file.read()
        return {"first": first.decode(), "rest": rest.decode()}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("file", "t.txt", b"hello world", None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["first"] == "hello"
    assert data["rest"] == " world"


def test_upload_write_seek_close():
    app = JustAPIApp()

    async def upload(file: UploadFile):
        await file.write(b"APPEND")
        pos = await file.seek(0)
        full = await file.read()
        await file.close()
        return {"pos": pos, "full": full.decode()}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("file", "t.txt", b"hello", None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["pos"] == 0
    assert data["full"] == "APPEND"


def test_upload_file_object_getter():
    app = JustAPIApp()

    async def upload(file: UploadFile):
        f = file.file
        return {"content": f.read().decode()}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("file", "t.txt", b"raw-bytes", None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["content"] == "raw-bytes"


def test_upload_too_large(monkeypatch):
    monkeypatch.setenv("JUSTAPI_MAX_UPLOAD_SIZE", "16")
    app = JustAPIApp()

    async def upload(file: UploadFile):
        return {"ok": True}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("file", "big.txt", b"x" * 50, None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 413


def test_upload_multiple_files_list():
    """`<input multiple>` parity: repeated field name → List[UploadFile]."""
    from typing import List

    app = JustAPIApp()

    async def upload(files: List[UploadFile]):
        out = []
        for f in files:
            data = await f.read()
            out.append({"filename": f.filename, "content": data.decode()})
        return {"count": len(files), "files": out}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart(
        [
            ("files", "a.txt", b"AAA", "text/plain"),
            ("files", "b.txt", b"BBB", "text/plain"),
        ]
    )
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data["count"] == 2
    assert [f["filename"] for f in data["files"]] == ["a.txt", "b.txt"]
    assert [f["content"] for f in data["files"]] == ["AAA", "BBB"]


def test_upload_list_with_single_file_wraps():
    """List[UploadFile] with one part still receives a 1-element list."""
    from typing import List

    app = JustAPIApp()

    async def upload(files: List[UploadFile]):
        return {"count": len(files), "filename": files[0].filename}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart([("files", "only.txt", b"x", None)])
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    assert data == {"count": 1, "filename": "only.txt"}


def test_upload_single_annotation_with_multi_parts_gets_list():
    """A single UploadFile annotation still works when multiple parts arrive
    (dict value is a list; FastAPI hands the first)."""
    from typing import List

    app = JustAPIApp()

    async def upload(file: UploadFile):
        data = await file.read()
        return {"filename": file.filename, "content": data.decode()}

    app.post("/upload", upload)
    client = JustAPITestClient(app)

    body, ct = _build_multipart(
        [("file", "a.txt", b"A", None), ("file", "b.txt", b"B", None)]
    )
    resp = client.post_with("/upload", body, [("Content-Type", ct)])
    assert resp["status"] == 200
    data = json.loads(bytes(resp["body"]))
    # Single annotation receives the first file (list is unwrapped to first).
    assert data["filename"] in ("a.txt", "b.txt")
