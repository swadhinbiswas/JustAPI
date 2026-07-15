"""JustAPI agent-native workload: request validation runs in Rust (jsonschema)."""

from justapi import JustAPIApp
import json

app = JustAPIApp()

USER_SCHEMA = {
    "type": "object",
    "properties": {
        "user": {
            "type": "object",
            "properties": {"name": {"type": "string"}, "id": {"type": "integer"}},
            "required": ["name", "id"],
        },
        "items": {"type": "array", "items": {"type": "integer"}},
        "meta": {"type": "object", "properties": {"version": {"type": "string"}}},
    },
    "required": ["user", "items", "meta"],
}


@app.post("/validate", body_schema=USER_SCHEMA)
def validate(request):
    data = json.loads(request["body"].decode("utf-8"))
    return data


if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8080"
    app.run(f"127.0.0.1:{port}")
