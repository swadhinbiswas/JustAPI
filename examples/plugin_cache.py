from justapi import JustAPIApp

class RedisCachePlugin:
    """A simple plugin to cache responses in an in-memory dict (simulating Redis)."""
    def __init__(self):
        self.cache = {}

    def build(self, app: JustAPIApp):
        print("[RedisCachePlugin] Building plugin...")
        # In a real app we might inject this cache into the app state

    async def on_startup(self):
        print("[RedisCachePlugin] Connecting to simulated Redis...")
        self.cache.clear()

    async def on_shutdown(self):
        print("[RedisCachePlugin] Disconnecting simulated Redis...")
        self.cache.clear()

    def get(self, key):
        return self.cache.get(key)
        
    def set(self, key, value):
        self.cache[key] = value

if __name__ == "__main__":
    app = JustAPIApp()
    cache_plugin = RedisCachePlugin()
    app.use(cache_plugin)

    def cached_hello(request):
        key = "hello"
        if cached := cache_plugin.get(key):
            return {"message": cached, "cached": True}
            
        value = "world"
        cache_plugin.set(key, value)
        return {"message": value, "cached": False}

    app.get("/hello", cached_hello)
    print("Run this file using a python runner to see it in action.")
