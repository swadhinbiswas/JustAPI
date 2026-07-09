import asyncio
import threading

_loop = asyncio.new_event_loop()

def start_loop(loop):
    asyncio.set_event_loop(loop)
    loop.run_forever()

t = threading.Thread(target=start_loop, args=(_loop,), daemon=True)
t.start()

async def my_coro():
    return 42

future = asyncio.run_coroutine_threadsafe(my_coro(), _loop)
print(future.result())
