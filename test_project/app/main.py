"""JustAPI application — test_project
"""

from justapi import JustAPIApp

app = JustAPIApp()


@app.get("/")
async def root():
    return {"message": "Hello from test_project!"}


@app.get("/health")
async def health():
    return {"status": "healthy"}


if __name__ == "__main__":
    app.run()
