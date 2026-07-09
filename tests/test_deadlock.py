from justapi import JustAPIApp
import asyncio
app = JustAPIApp()
async def hello(req):
    await asyncio.sleep(0.01)
    return {"message": "hello"}
app.get("/", hello)
if __name__ == "__main__":
    app.run("127.0.0.1:8080")
