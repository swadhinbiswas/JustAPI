from justapi import JustAPIApp, Depends

app = JustAPIApp()

def common_parameters(q: str | None = None, skip: int = 0, limit: int = 100):
    return {"q": q, "skip": skip, "limit": limit}

@app.get("/items/")
def read_items(commons: dict = Depends(common_parameters)):
    return {"message": "Reading items", "params": commons}

@app.get("/users/")
def read_users(commons: dict = Depends(common_parameters)):
    return {"message": "Reading users", "params": commons}

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
