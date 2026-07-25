use std::collections::HashMap;
use std::sync::Arc;

use minijinja::Environment;
use tokio::sync::RwLock;

/// Renders email templates using MiniJinja.
pub struct EmailTemplates {
    env: Arc<RwLock<Environment<'static>>>,
}

impl std::fmt::Debug for EmailTemplates {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("EmailTemplates").finish()
    }
}

impl EmailTemplates {
    pub fn new() -> Self {
        Self { env: Arc::new(RwLock::new(Environment::new())) }
    }

    /// Add a template from a string.
    pub async fn add_template(&self, name: &str, template: &str) -> Result<(), anyhow::Error> {
        let mut env = self.env.write().await;
        env.add_template_owned(name.to_string(), template.to_string())?;
        Ok(())
    }

    /// Render a named template with the given context.
    pub async fn render(
        &self,
        name: &str,
        ctx: &HashMap<String, serde_json::Value>,
    ) -> Result<String, anyhow::Error> {
        let env = self.env.read().await;
        let tmpl = env.get_template(name)?;
        let result = tmpl.render(ctx)?;
        Ok(result)
    }
}

impl Default for EmailTemplates {
    fn default() -> Self {
        Self::new()
    }
}
