from justapi import JustAPIApp

app = JustAPIApp()

@app.get("/")
def read_root():
    return {"Hello": "World"}

@app.get("/items/{item_id}")
def read_item(item_id: int):
    return {"item_id": item_id, "name": "Item Name"}

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
