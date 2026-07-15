from fastapi import FastAPI, Request
from pydantic import BaseModel
import json

app = FastAPI()


class Item(BaseModel):
    id: int
    name: str
    price: float


@app.post("/body_json")
async def body_json(request: Request):
    data = json.loads(await request.body())
    return {"ok": True}


@app.post("/validate")
async def validate(item: Item):
    return {"ok": True}


@app.post("/noop")
async def noop():
    return {"ok": True}
