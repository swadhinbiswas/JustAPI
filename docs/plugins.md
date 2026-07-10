# JustAPI Plugin System

JustAPI provides a powerful plugin system that allows you to extend the framework with custom functionality. Plugins can be written in either Rust (for maximum performance) or Python (for flexibility and ease of use).

## Python Plugins

Python plugins are the simplest way to extend JustAPI. A Python plugin is simply a class that implements one or more of the following hooks:

- `build(self, app)`: Called when the plugin is registered with `app.use()`. You can use this to add routes, middleware, or modify the app configuration.
- `on_startup(self)`: Called asynchronously just before the server starts accepting requests. Useful for establishing database connections, starting background tasks, or warming up caches.
- `on_shutdown(self)`: Called asynchronously when the server is shutting down. Useful for cleaning up resources, closing connections, or saving state.

### Example: Simple Authentication Plugin

```python
from justapi import JustAPIApp

class SimpleAuthPlugin:
    def __init__(self, token: str):
        self.expected_token = token

    def build(self, app: JustAPIApp):
        print("Auth plugin registered.")

    async def on_startup(self):
        print("Auth plugin initialized.")

    async def on_shutdown(self):
        print("Auth plugin shutting down.")
        
    def authenticate(self, request):
        token = request["headers"].get(b"authorization")
        if token != f"Bearer {self.expected_token}".encode():
            return {"status": 401, "body": b'{"error":"unauthorized"}'}
        return None

app = JustAPIApp()
auth = SimpleAuthPlugin(token="secret123")
app.use(auth)

# Use the plugin logic in a route
def secure_route(request):
    err = auth.authenticate(request)
    if err:
        return err
    return {"message": "secure data"}

app.get("/secure", secure_route)
```

## Rust Plugins

For performance-critical extensions (like custom protocols, low-level middleware, or custom allocators), you can write Rust plugins. Rust plugins must implement the `justapi_core::plugin::Plugin` trait and are statically registered using the `inventory` crate.

### The `Plugin` Trait

```rust
use justapi_core::plugin::Plugin;
use async_trait::async_trait;
use anyhow::Result;

pub struct MyRustPlugin;

#[async_trait]
impl Plugin for MyRustPlugin {
    fn name(&self) -> &'static str {
        "MyRustPlugin"
    }

    async fn on_startup(&self) -> Result<()> {
        println!("Rust plugin startup!");
        Ok(())
    }

    async fn on_shutdown(&self) -> Result<()> {
        println!("Rust plugin shutdown!");
        Ok(())
    }
}
```

### Registration

To register your Rust plugin, use `inventory::submit!`:

```rust
use justapi_core::plugin::PluginRegistration;
use std::sync::Arc;

inventory::submit! {
    PluginRegistration::new(|| Arc::new(MyRustPlugin))
}
```

When `Server::run()` is called, JustAPI will automatically discover all registered plugins and invoke their lifecycle hooks in order.

## Plugin Marketplace

To publish a JustAPI Python plugin to the broader community, simply publish it to PyPI with the `justapi-plugin` keyword.
Users can then install it via `pip install justapi-plugin-yourname` and use it as standard:

```python
from justapi_plugin_yourname import YourPlugin
app.use(YourPlugin())
```
