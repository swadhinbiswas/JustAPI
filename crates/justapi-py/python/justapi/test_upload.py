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
