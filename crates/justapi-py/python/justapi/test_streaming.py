"""Tests for streaming validated structured output and the agent session store."""

import json

import pytest

from justapi import JustAPIApp, JustAPITestClient, ValidatedStreamResponse
from justapi._justapi import validate_value


# --- Rust validator --------------------------------------------------------


def test_validate_value_ok():
    schema = json.dumps({"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"]})
    assert validate_value(schema, json.dumps({"n": 1})) == []


def test_validate_value_errors():
    schema = json.dumps({"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"]})
    errs = validate_value(schema, json.dumps({"n": "not-int"}))
    assert len(errs) >= 1
    # missing required -> error
    assert len(validate_value(schema, json.dumps({}))) >= 1


# --- Streaming validated output (NDJSON) -----------------------------------


def _make_stream_app():
    app = JustAPIApp()
    schema = {"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"]}

    @app.stream_json("/nums", schema=schema, mode="ndjson")
    def nums():
        for i in range(3):
            yield {"n": i}

    return app


def test_stream_ndjson_all_valid():
    app = _make_stream_app()
    r = JustAPITestClient(app).get("/nums")
    assert r["status"] == 200
    lines = [json.loads(l) for l in bytes(r["body"]).decode().splitlines() if l.strip()]
    assert lines == [{"n": 0}, {"n": 1}, {"n": 2}]


def test_stream_ndjson_aborts_on_invalid():
    app = JustAPIApp()
    schema = {"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"]}

    @app.stream_json("/nums", schema=schema, mode="ndjson")
    def nums():
        yield {"n": 1}
        yield {"n": "bad"}  # invalid -> stream must abort here
        yield {"n": 3}      # never emitted

    r = JustAPITestClient(app).get("/nums")
    lines = [json.loads(l) for l in bytes(r["body"]).decode().splitlines() if l.strip()]
    assert {"n": 1} in lines
    # invalid + everything after it is never sent
    assert {"n": "bad"} not in lines
    assert {"n": 3} not in lines


def test_stream_array_mode():
    app = JustAPIApp()
    schema = {"type": "object", "properties": {"n": {"type": "integer"}}, "required": ["n"]}

    @app.stream_json("/nums", schema=schema, mode="array")
    def nums():
        for i in range(2):
            yield {"n": i}

    r = JustAPITestClient(app).get("/nums")
    assert json.loads(bytes(r["body"])) == [{"n": 0}, {"n": 1}]


# --- Agent session state ---------------------------------------------------


def test_session_crud():
    app = JustAPIApp()
    sid = app.create_session({"count": 0})
    assert app.get_session(sid) == {"count": 0}

    assert app.update_session(sid, count=1, name="x") is True
    assert app.get_session(sid) == {"count": 1, "name": "x"}

    assert app.set_session(sid, {"reset": True}) is True
    assert app.get_session(sid) == {"reset": True}

    assert app.delete_session(sid) is True
    assert app.get_session(sid) is None
    # updating a missing session returns False
    assert app.update_session("nope", x=1) is False


def test_session_injection():
    app = JustAPIApp()
    from justapi import Session

    sid = app.create_session({"visits": 0})

    @app.get("/visit")
    def visit(session: Session):
        session.update(visits=session.get()["visits"] + 1)
        return {"visits": session.get()["visits"], "id": session.id}

    r = JustAPITestClient(app).get(f"/visit?session={sid}")
    assert r["status"] == 200
    body = json.loads(bytes(r["body"]))
    assert body["visits"] == 1
    assert body["id"] == sid
    # state persisted in the Rust store
    assert app.get_session(sid) == {"visits": 1}
