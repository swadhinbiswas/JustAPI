use std::collections::HashMap;

use anyhow::Result;
use async_trait::async_trait;

use crate::Server;

/// Lifecycle trait for JustAPI plugins.
/// Plugins can hook into the server lifecycle and modify the application setup.
#[async_trait]
pub trait Plugin: Send + Sync {
    /// Unique name of the plugin.
    fn name(&self) -> &'static str;

    /// Called when the plugin is registered with the server.
    /// The plugin can register middlewares, routers, or modify the Server configuration here.
    fn build(&self, _server: &mut Server) -> Result<()> {
        Ok(())
    }

    /// Called when the server is starting up.
    async fn on_startup(&self) -> Result<()> {
        Ok(())
    }

    /// Called when the server is shutting down.
    async fn on_shutdown(&self) -> Result<()> {
        Ok(())
    }
}

/// A registration entry for a plugin. This is used by the `inventory` crate
/// to collect statically registered plugins at compile time.
pub struct PluginRegistration {
    pub name: &'static str,
    pub builder: fn() -> Box<dyn Plugin>,
}

inventory::collect!(PluginRegistration);

/// A registry of loaded plugins.
#[derive(Default)]
pub struct PluginRegistry {
    plugins: HashMap<String, Box<dyn Plugin>>,
}

impl PluginRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Register a plugin instance explicitly.
    pub fn register(&mut self, plugin: Box<dyn Plugin>) {
        self.plugins.insert(plugin.name().to_string(), plugin);
    }

    /// Load all plugins that were registered statically via the `inventory` crate.
    pub fn load_static_plugins(&mut self) {
        for reg in inventory::iter::<PluginRegistration> {
            let plugin = (reg.builder)();
            self.plugins.insert(plugin.name().to_string(), plugin);
        }
    }

    /// Invoke the `build` lifecycle hook on all registered plugins.
    pub fn build_all(&self, server: &mut Server) -> Result<()> {
        for plugin in self.plugins.values() {
            plugin.build(server)?;
        }
        Ok(())
    }

    /// Invoke the `on_startup` lifecycle hook on all registered plugins.
    pub async fn on_startup_all(&self) -> Result<()> {
        for plugin in self.plugins.values() {
            plugin.on_startup().await?;
        }
        Ok(())
    }

    /// Invoke the `on_shutdown` lifecycle hook on all registered plugins.
    pub async fn on_shutdown_all(&self) -> Result<()> {
        for plugin in self.plugins.values() {
            plugin.on_shutdown().await?;
        }
        Ok(())
    }
}
