import json
import asyncio
import threading
from justapi import JustAPIApp

class MyPlugin:
    def __init__(self):
        self.built = False
        self.started = False
        self.shutdown = False

    def build(self, app: JustAPIApp):
        self.built = True
        app.get("/from-plugin", self.handler)

    def on_startup(self):
        self.started = True

    async def on_shutdown(self):
        self.shutdown = True

    def handler(self, request):
        return {"message": "hello from plugin"}

def test_plugin_registration():
    app = JustAPIApp()
    plugin = MyPlugin()
    app.use(plugin)

    assert plugin.built is True
    # The plugin was appended to app's plugins list.
    
    # We can't easily test on_startup/on_shutdown without running the server, 
    # but we can test if the route was registered correctly using the TestClient.
    from justapi import JustAPITestClient
    client = JustAPITestClient(app)
    
    resp = client.get("/from-plugin")
    assert resp["status"] == 200
    assert json.loads(bytes(resp["body"])) == {"message": "hello from plugin"}

def test_plugin_hooks_in_real_server():
    app = JustAPIApp()
    plugin = MyPlugin()
    app.use(plugin)

    # Run the server in a thread
    def run_server():
        try:
            app.run("127.0.0.1:0")
        except Exception:
            pass

    server_thread = threading.Thread(target=run_server, daemon=True)
    server_thread.start()

    # Give it a tiny bit of time to start
    import time
    time.sleep(0.5)

    assert plugin.started is True
    assert plugin.shutdown is False
    
    # We can't cleanly test shutdown hook here because shutting down
    # requires a ctrl-c signal or something that our test runner might not
    # want to handle gracefully for a daemon thread, but we tested startup!
