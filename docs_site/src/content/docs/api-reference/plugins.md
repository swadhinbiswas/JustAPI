---
title: Plugin System API
description: Reference for extending JustAPI with Python and Rust plugins.
---

JustAPI's plugin system supports Python plugins (for flexibility) and Rust plugins (for maximum performance).

## Python Plugin Interface

A Python plugin is any class implementing one or more lifecycle hooks:

```python
class MyPlugin:
    def build(self, app):
        """Called when plugin is registered with app.use()."""
        pass

    async def on_startup(self):
        """Called before server starts accepting requests."""
        pass

    async def on_shutdown(self):
        """Called during server shutdown."""
        pass
```

### Hook Details

| Method | Timing | Purpose |
|---|---|---|
| `build(self, app)` | On `app.use()` | Add routes, middleware, modify config |
| `on_startup(self)` | Before server starts | Connect to services, warm caches |
| `on_shutdown(self)` | During graceful shutdown | Close connections, save state |

### Registration

```python
from justapi import JustAPIApp

class MyPlugin:
    def build(self, app: JustAPIApp):
        print("Plugin registered!")

    async def on_startup(self):
        print("Server starting...")

    async def on_shutdown(self):
        print("Server stopping...")

app = JustAPIApp()
app.use(MyPlugin())
```

## Rust Plugin Interface

For performance-critical extensions, Rust plugins implement the `Plugin` trait:

```rust
use justapi_core::plugin::Plugin;
use async_trait::async_trait;

pub struct MyRustPlugin;

#[async_trait]
impl Plugin for MyRustPlugin {
    fn name(&self) -> &'static str {
        "MyRustPlugin"
    }

    async fn on_startup(&self) -> anyhow::Result<()> {
        println!("Rust plugin started");
        Ok(())
    }

    async fn on_shutdown(&self) -> anyhow::Result<()> {
        println!("Rust plugin stopped");
        Ok(())
    }
}
```

### Registration

```rust
use justapi_core::plugin::PluginRegistration;
use std::sync::Arc;

inventory::submit! {
    PluginRegistration::new(|| Arc::new(MyRustPlugin))
}
```

## Plugin Marketplace

To publish a JustAPI Python plugin to the community:

1. Publish to PyPI with the `justapi-plugin` keyword
2. Users install via `pip install justapi-plugin-yourname`
3. Usage: `from justapi_plugin_yourname import YourPlugin; app.use(YourPlugin())`

## See Also

- [Plugins Guide](/docs/plugins.md) — Detailed plugin development guide
- [JustAPIApp](/api-reference/justapiapp/) — `app.use()` method
- [Rust Core Deep Dive](/advanced/rust-core-deep-dive/) — Plugin trait internals
