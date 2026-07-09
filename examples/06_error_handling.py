from justapi import JustAPIApp, HTTPException, RequestValidationError

app = JustAPIApp()

@app.exception_handler(HTTPException)
def http_exception_handler(request, exc: HTTPException):
    return {
        "status": exc.status_code,
        "body": {"error": "Custom Error Format", "detail": exc.detail},
    }

@app.exception_handler(RequestValidationError)
def validation_exception_handler(request, exc: RequestValidationError):
    return {
        "status": 422,
        "body": {"error": "Validation Failed", "issues": exc.errors()},
    }

@app.get("/items/{item_id}")
def read_item(item_id: int):
    if item_id == 3:
        raise HTTPException(status_code=418, detail="I'm a teapot")
    return {"item_id": item_id}

if __name__ == "__main__":
    app.run("127.0.0.1:8000")
