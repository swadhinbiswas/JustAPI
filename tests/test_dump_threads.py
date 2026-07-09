from justapi import JustAPIApp
import asyncio, threading, sys, traceback

app = JustAPIApp()

async def hello(req):
    await asyncio.sleep(0.01)
    return {"message": "hello"}

app.get("/", hello)

def dump_threads():
    import time
    while True:
        time.sleep(2)
        print("--- THREAD DUMP ---")
        for th in threading.enumerate():
            print(th)
            traceback.print_stack(sys._current_frames()[th.ident])
        print("-------------------")

threading.Thread(target=dump_threads, daemon=True).start()

if __name__ == "__main__":
    app.run("127.0.0.1:8080")
