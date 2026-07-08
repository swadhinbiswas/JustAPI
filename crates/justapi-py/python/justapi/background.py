import inspect
import asyncio
import threading

class BackgroundTasks:
    """
    Collects functions to execute after a response is returned.
    """
    def __init__(self):
        self.tasks = []

    def add_task(self, func, *args, **kwargs):
        self.tasks.append((func, args, kwargs))

    def __call__(self):
        """Execute all background tasks."""
        for func, args, kwargs in self.tasks:
            if inspect.iscoroutinefunction(func):
                # Try to get the current event loop, otherwise start a new thread for it
                try:
                    loop = asyncio.get_running_loop()
                    loop.create_task(func(*args, **kwargs))
                except RuntimeError:
                    # If no running loop, run in a new thread with its own loop
                    def run_async(f, a, kw):
                        new_loop = asyncio.new_event_loop()
                        asyncio.set_event_loop(new_loop)
                        new_loop.run_until_complete(f(*a, **kw))
                    threading.Thread(target=run_async, args=(func, args, kwargs)).start()
            else:
                # Run sync functions in a background thread
                threading.Thread(target=func, args=args, kwargs=kwargs).start()
