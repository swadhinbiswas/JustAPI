"""Robyn agent-native workload: request validation runs in Python (pydantic)."""

from robyn import Robyn
from pydantic import BaseModel, Field

app = Robyn(__file__)


class User(BaseModel):
    name: str
    id: int


class Meta(BaseModel):
    version: str


class Payload(BaseModel):
    user: User
    items: list[int]
    meta: Meta


@app.post("/validate")
def validate(request):
    data = Payload.model_validate(request.json())
    return data.model_dump()


if __name__ == "__main__":
    import sys
    port = sys.argv[1] if len(sys.argv) > 1 else "8080"
    app.start(host="127.0.0.1", port=int(port))
