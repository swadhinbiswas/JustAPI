use arc_swap::ArcSwap;
use hyper::Method;
use notify::{Config, Event, RecursiveMode, Watcher};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::router::Router;

use crate::middleware::{Middleware, Next};
use crate::ResponseBody;
use async_trait::async_trait;
use http_body_util::BodyExt;
use hyper::{Request, Response};

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GatewayRoute {
    pub upstream: String,
    pub rate_limit_capacity: Option<u64>,
    pub rate_limit_replenish_rate: Option<u64>,
    pub wasm_plugin: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize, Default)]
pub struct GatewayConfigData {
    pub routes: HashMap<String, HashMap<String, GatewayRoute>>, // path -> { method -> route }
    pub global_rate_limit: Option<u64>,
}

#[derive(Clone)]
pub struct GatewayRouter {
    pub inner: Router<GatewayRoute>,
}

pub struct GatewayState {
    pub config: ArcSwap<GatewayConfigData>,
    pub router: ArcSwap<GatewayRouter>,
    config_path: PathBuf,
}

impl GatewayState {
    pub fn new(path: impl AsRef<Path>) -> Arc<Self> {
        let path = path.as_ref().to_path_buf();
        let (config, router) = Self::load_config(&path).unwrap_or_else(|e| {
            warn!("Failed to load gateway config from {:?}: {}", path, e);
            (GatewayConfigData::default(), GatewayRouter { inner: Router::new() })
        });

        Arc::new(Self {
            config: ArcSwap::from_pointee(config),
            router: ArcSwap::from_pointee(router),
            config_path: path,
        })
    }

    fn load_config(path: &Path) -> anyhow::Result<(GatewayConfigData, GatewayRouter)> {
        let data = std::fs::read_to_string(path)?;
        let config: GatewayConfigData = serde_json::from_str(&data)?;

        let mut router = Router::new();
        for (path_pattern, methods) in &config.routes {
            for (method_str, route) in methods {
                let method = match method_str.to_uppercase().as_str() {
                    "GET" => Method::GET,
                    "POST" => Method::POST,
                    "PUT" => Method::PUT,
                    "DELETE" => Method::DELETE,
                    "PATCH" => Method::PATCH,
                    "OPTIONS" => Method::OPTIONS,
                    _ => continue,
                };
                let _ = router.insert(method, path_pattern, route.clone());
            }
        }

        Ok((config, GatewayRouter { inner: router }))
    }

    pub fn watch(self: Arc<Self>) -> anyhow::Result<()> {
        let (tx, mut rx) = mpsc::channel(10);

        let mut watcher = notify::RecommendedWatcher::new(
            move |res: Result<Event, notify::Error>| {
                if let Ok(event) = res {
                    if event.kind.is_modify() || event.kind.is_create() {
                        let _ = tx.blocking_send(());
                    }
                }
            },
            Config::default(),
        )?;

        watcher.watch(&self.config_path, RecursiveMode::NonRecursive)?;

        tokio::spawn(async move {
            // Keep watcher alive by moving it into this task
            let _watcher = watcher;

            while rx.recv().await.is_some() {
                // Debounce
                tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

                info!("Gateway configuration file changed, reloading...");
                match Self::load_config(&self.config_path) {
                    Ok((config, router)) => {
                        self.config.store(Arc::new(config));
                        self.router.store(Arc::new(router));
                        info!("Gateway configuration reloaded successfully.");
                    }
                    Err(e) => {
                        error!("Failed to reload gateway configuration: {}", e);
                    }
                }
            }
        });

        Ok(())
    }

    pub fn get_route<'a, 'p>(
        &'a self,
        _method: &Method,
        _path: &'p str,
    ) -> Option<crate::router::Match<'a, 'p, GatewayRoute>> {
        let _current_router = self.router.load();
        // Since we are borrowing from `current_router` which is an Arc guard, we need to return owned or clone if we want to drop the guard.
        // For simplicity, we can just return a clone of the `GatewayRoute` and its params.
        None // Wait, we will implement this inside the Middleware where the guard lives.
    }
}

pub struct GatewayMiddleware {
    state: Arc<GatewayState>,
}

impl GatewayMiddleware {
    pub fn new(state: Arc<GatewayState>) -> Self {
        Self { state }
    }
}

#[async_trait]
impl<B: Send + 'static> Middleware<B> for GatewayMiddleware {
    async fn handle(
        &self,
        req: Request<B>,
        next: Next<'_, B>,
    ) -> anyhow::Result<Response<ResponseBody>> {
        let path = req.uri().path().to_string();
        let method = req.method().clone();

        // Acquire RCU read guard (wait-free, extremely fast)
        let router_guard = self.state.router.load();

        // Check if Gateway routing handles this
        if let Ok(matched) = router_guard.inner.at(&method, &path) {
            let route = matched.handler;
            info!("Gateway intercepted route to upstream: {}", route.upstream);

            // Example proxy logic here. For now, just return a mock response showing hot-reloaded proxy!
            use http_body_util::Full;
            use hyper::body::Bytes;

            let body = Full::new(Bytes::from(format!("Proxied to: {}", route.upstream)))
                .map_err(|never| match never {})
                .boxed_unsync();

            return Ok(Response::builder()
                .status(200)
                .header("x-justapi-gateway", "hot-reloaded")
                .body(body)
                .unwrap());
        }

        // Fallback to next (e.g. Python app)
        next.run(req).await
    }
}
