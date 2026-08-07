use std::collections::{HashMap, VecDeque};
use std::sync::{Arc, Mutex};

use hyper::Method;
use matchit::Router as MatchitRouter;

pub use matchit::{InsertError, MatchError};

#[derive(Debug, PartialEq, Eq)]
pub enum RouterError {
    NotFound,
    MethodNotAllowed,
}

#[derive(Debug, Clone)]
pub struct Match<'a, 'p, T> {
    pub handler: &'a T,
    pub params: matchit::Params<'a, 'p>,
}

/// An owned route resolution (no borrows into the router), used by the hot
/// request path and by the route cache.
#[derive(Debug, Clone)]
pub struct RouteResolution<T> {
    pub handler: T,
    pub params: Vec<(String, String)>,
}

/// Cache entry for a `(Method, path)` lookup. We only cache the two stable
/// outcomes: a static-route hit (no path params) and a definitive `NotFound`.
/// Param routes and `MethodNotAllowed` are never cached — they require re-running
/// the full match (param extraction / cross-method scan) on every request.
#[derive(Debug, Clone)]
enum CacheEntry<T> {
    Hit(RouteResolution<T>),
    NotFound,
}

/// Bounded route-lookup cache. A single `Mutex<HashMap>` with embedded FIFO
/// eviction avoids the deadlock-prone dual-mutex approach.
#[derive(Debug)]
struct RouteCache<T> {
    inner: Mutex<CacheInner<T>>,
    capacity: usize,
}

#[derive(Debug)]
struct CacheInner<T> {
    map: HashMap<(Method, String), CacheEntry<T>>,
    order: VecDeque<(Method, String)>,
}

impl<T: Clone> RouteCache<T> {
    fn new(capacity: usize) -> Self {
        Self {
            inner: Mutex::new(CacheInner {
                map: HashMap::with_capacity(capacity),
                order: VecDeque::with_capacity(capacity),
            }),
            capacity: capacity.max(1),
        }
    }

    /// Look up a cached resolution.
    fn get(&self, key: &(Method, String)) -> Option<CacheEntry<T>> {
        let inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        inner.map.get(key).cloned()
    }

    /// Insert a cacheable entry, evicting the oldest key if at capacity.
    fn insert(&self, key: (Method, String), entry: CacheEntry<T>) {
        let mut inner = self.inner.lock().unwrap_or_else(|e| e.into_inner());
        if !inner.map.contains_key(&key) {
            inner.order.push_back(key.clone());
            if inner.order.len() > self.capacity {
                if let Some(old) = inner.order.pop_front() {
                    inner.map.remove(&old);
                }
            }
        }
        inner.map.insert(key, entry);
    }
}

#[derive(Debug, Clone)]
pub struct Router<T> {
    routes: HashMap<Method, MatchitRouter<T>>,
    fallback: Option<T>,
    route_list: Vec<(Method, String)>,
    /// Optional per-request route-lookup cache. `None` disables caching.
    cache: Option<Arc<RouteCache<T>>>,
}

impl<T: Clone> Router<T> {
    /// Create a router with an always-on route cache (default capacity 16384).
    pub fn new() -> Self {
        Self {
            routes: HashMap::new(),
            fallback: None,
            route_list: Vec::new(),
            cache: Some(Arc::new(RouteCache::new(16384))),
        }
    }

    /// Create a router without a route cache.
    pub fn without_cache() -> Self {
        Self { routes: HashMap::new(), fallback: None, route_list: Vec::new(), cache: None }
    }

    /// Replace the route cache with one sized to `capacity` (0 disables).
    pub fn with_cache_capacity(mut self, capacity: usize) -> Self {
        self.cache = if capacity == 0 { None } else { Some(Arc::new(RouteCache::new(capacity))) };
        self
    }

    pub fn insert(&mut self, method: Method, path: &str, handler: T) -> Result<(), InsertError> {
        let is_get = method == Method::GET;
        let router = self.routes.entry(method.clone()).or_default();
        router.insert(path, handler.clone())?;
        self.route_list.push((method, path.to_string()));

        if is_get {
            let head_router = self.routes.entry(Method::HEAD).or_default();
            let _ = head_router.insert(path, handler);
            self.route_list.push((Method::HEAD, path.to_string()));
        }
        Ok(())
    }

    /// List all registered routes as `(method, path)` pairs.
    pub fn list_routes(&self) -> &[(Method, String)] {
        &self.route_list
    }

    pub fn at<'a, 'p>(
        &'a self,
        method: &Method,
        path: &'p str,
    ) -> Result<Match<'a, 'p, T>, RouterError> {
        if let Some(router) = self.routes.get(method) {
            if let Ok(matched) = router.at(path) {
                return Ok(Match { handler: matched.value, params: matched.params });
            }
        }

        for (other_method, router) in &self.routes {
            if other_method != method && router.at(path).is_ok() {
                return Err(RouterError::MethodNotAllowed);
            }
        }

        Err(RouterError::NotFound)
    }

    /// Shared match logic returning an owned resolution (no borrows).
    fn raw_resolve(&self, method: &Method, path: &str) -> Result<RouteResolution<T>, RouterError> {
        if let Some(router) = self.routes.get(method) {
            if let Ok(matched) = router.at(path) {
                let params = matched
                    .params
                    .iter()
                    .map(|(k, v)| (k.to_string(), v.to_string()))
                    .collect::<Vec<_>>();
                return Ok(RouteResolution { handler: matched.value.clone(), params });
            }
        }

        for (other_method, router) in &self.routes {
            if other_method != method && router.at(path).is_ok() {
                return Err(RouterError::MethodNotAllowed);
            }
        }

        Err(RouterError::NotFound)
    }

    /// Resolve a route to an owned `RouteResolution`, memoized through the
    /// optional route cache. Static-route hits and definitive `NotFound`s are
    /// cached; param routes and `MethodNotAllowed` bypass the cache (they need
    /// a fresh full match each time).
    pub fn resolve(&self, method: &Method, path: &str) -> Result<RouteResolution<T>, RouterError> {
        if let Some(cache) = &self.cache {
            let key = (method.clone(), path.to_string());
            if let Some(entry) = cache.get(&key) {
                return match entry {
                    CacheEntry::Hit(res) => Ok(res),
                    CacheEntry::NotFound => Err(RouterError::NotFound),
                };
            }
            let res = self.raw_resolve(method, path);
            match &res {
                Ok(r) if r.params.is_empty() => {
                    cache.insert(key, CacheEntry::Hit(r.clone()));
                }
                Err(RouterError::NotFound) => {
                    cache.insert(key, CacheEntry::NotFound);
                }
                _ => {}
            }
            res
        } else {
            self.raw_resolve(method, path)
        }
    }

    pub fn set_fallback(&mut self, handler: T) {
        self.fallback = Some(handler);
    }

    pub fn fallback(&self) -> Option<&T> {
        self.fallback.as_ref()
    }
}

impl<T: Clone> Default for Router<T> {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_basic_route() {
        let mut router = Router::new();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        let m = router.at(&Method::GET, "/hello").unwrap();
        assert_eq!(*m.handler, "hello_handler");
    }

    #[test]
    fn test_resolve_static_hit_and_cache() {
        let mut router = Router::new();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        let r = router.resolve(&Method::GET, "/hello").unwrap();
        assert_eq!(r.handler, "hello_handler");
        assert!(r.params.is_empty());
        // Second call must hit the cache and return the same handler.
        let r2 = router.resolve(&Method::GET, "/hello").unwrap();
        assert_eq!(r2.handler, "hello_handler");
    }

    #[test]
    fn test_resolve_not_found_is_cached() {
        let mut router = Router::new();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        assert!(router.resolve(&Method::GET, "/missing").is_err());
        // Negative result is memoized; still NotFound on repeat.
        assert!(router.resolve(&Method::GET, "/missing").is_err());
    }

    #[test]
    fn test_resolve_param_route_bypasses_cache() {
        let mut router = Router::new();
        router.insert(Method::GET, "/users/{id}", "user_handler").unwrap();
        let r = router.resolve(&Method::GET, "/users/42").unwrap();
        assert_eq!(r.handler, "user_handler");
        assert_eq!(r.params, vec![("id".to_string(), "42".to_string())]);
    }

    #[test]
    fn test_without_cache_still_resolves() {
        let mut router = Router::without_cache();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        let r = router.resolve(&Method::GET, "/hello").unwrap();
        assert_eq!(r.handler, "hello_handler");
    }

    #[test]
    fn test_route_not_found() {
        let mut router = Router::new();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        assert!(router.at(&Method::GET, "/unknown").is_err());
    }

    #[test]
    fn test_wrong_method() {
        let mut router = Router::new();
        router.insert(Method::GET, "/hello", "hello_handler").unwrap();
        assert_eq!(
            router.at(&Method::POST, "/hello").err().unwrap(),
            RouterError::MethodNotAllowed
        );
    }

    #[test]
    fn test_path_param() {
        let mut router = Router::new();
        router.insert(Method::GET, "/users/{id}", "user_handler").unwrap();
        let m = router.at(&Method::GET, "/users/42").unwrap();
        assert_eq!(*m.handler, "user_handler");
        assert_eq!(m.params.get("id"), Some("42"));
    }

    #[test]
    fn test_catch_all() {
        let mut router = Router::new();
        router.insert(Method::GET, "/files/{*path}", "file_handler").unwrap();
        let m = router.at(&Method::GET, "/files/a/b/c").unwrap();
        assert_eq!(*m.handler, "file_handler");
        assert_eq!(m.params.get("path"), Some("a/b/c"));
    }

    #[test]
    fn test_multiple_routes() {
        let mut router = Router::new();
        router.insert(Method::GET, "/users/{id}", "user_handler").unwrap();
        router.insert(Method::GET, "/users", "users_list").unwrap();
        let m = router.at(&Method::GET, "/users/42").unwrap();
        assert_eq!(*m.handler, "user_handler");
        let m = router.at(&Method::GET, "/users").unwrap();
        assert_eq!(*m.handler, "users_list");
    }

    #[test]
    fn test_post_route() {
        let mut router = Router::new();
        router.insert(Method::POST, "/data", "data_handler").unwrap();
        let m = router.at(&Method::POST, "/data").unwrap();
        assert_eq!(*m.handler, "data_handler");
    }

    #[test]
    fn test_query_method_route() {
        let mut router = Router::new();
        router.insert(crate::query_method(), "/search", "search_handler").unwrap();
        let m = router.at(&crate::query_method(), "/search").unwrap();
        assert_eq!(*m.handler, "search_handler");
        // QUERY is distinct from POST on the same path.
        assert_eq!(
            router.at(&Method::POST, "/search").err().unwrap(),
            RouterError::MethodNotAllowed
        );
    }

    #[test]
    fn test_fallback() {
        let mut router = Router::new();
        router.set_fallback("not_found");
        assert_eq!(*router.fallback().unwrap(), "not_found");
    }

    #[test]
    fn test_route_conflict() {
        let mut router = Router::new();
        router.insert(Method::GET, "/users/{id}", "a").unwrap();
        let err = router.insert(Method::GET, "/users/{name}", "b");
        assert!(err.is_err());
    }

    #[test]
    fn bench_500_route_lookup() {
        let mut router = Router::new();
        // Insert 500 routes with various patterns
        for i in 0..500 {
            router
                .insert(
                    Method::GET,
                    &format!("/api/v{}/users/{}/posts/{}", i % 5, i, i * 2),
                    format!("handler_{}", i),
                )
                .unwrap();
        }
        // Add some parameterized routes
        for i in 0..100 {
            router
                .insert(
                    Method::GET,
                    &format!("/api/users/{{id}}/posts/{{post_id}}/comments/{}", i),
                    format!("param_handler_{}", i),
                )
                .unwrap();
        }
        // Catch-all
        router.insert(Method::GET, "/static/{*path}", "static_handler".to_string()).unwrap();

        // Warmup
        for _ in 0..1000 {
            let _ = router.at(&Method::GET, "/api/v3/users/42/posts/84");
        }

        let iterations = if cfg!(miri) { 50u64 } else { 100_000u64 };
        let mut total_ns = 0u64;

        let test_paths = [
            "/api/v3/users/42/posts/84",
            "/api/v0/users/100/posts/200",
            "/api/users/999/posts/888/comments/50",
            "/static/css/main.css",
            "/unknown/path",
        ];

        for _ in 0..iterations {
            for path in &test_paths {
                let start = std::time::Instant::now();
                let _ = router.at(&Method::GET, path);
                total_ns += start.elapsed().as_nanos() as u64;
            }
        }

        let total_lookups = iterations * test_paths.len() as u64;
        let avg_ns = total_ns / total_lookups;
        println!("500-route table: {} lookups, avg {}ns", total_lookups, avg_ns,);

        // Benchmark gate: average lookup should be under 2us (release) / 5us (debug)
        // Skip threshold check in Miri interpreter mode (sanitizer runs filter
        // bench tests out via scripts/sanitize.sh — see below).
        if !cfg!(miri) {
            let target = if cfg!(debug_assertions) { 5000 } else { 2000 };
            assert!(avg_ns < target, "Route lookup too slow: {}ns (target < {}ns)", avg_ns, target);
        }
    }
}
