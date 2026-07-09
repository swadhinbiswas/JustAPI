from justapi import JustAPIApp
import asyncio, threading

app = JustAPIApp()

async def hello(req):
    await asyncio.sleep(0.01)
    return {"message": "hello"}

app.get("/", hello)

def monitor():
    import time
    while True:
        time.sleep(1)
        print("Threads:", threading.enumerate())

threading.Thread(target=monitor, daemon=True).start()

if __name__ == "__main__":
    app.run("127.0.0.1:8080")
