# JustAPI Troubleshooting Guide

Common issues and solutions for JustAPI deployments.

---

## Table of Contents

1. [GIL Deadlock](#1-gil-deadlock)
2. [Connection Pool Exhaustion](#2-connection-pool-exhaustion)
3. [TLS Certificate Issues](#3-tls-certificate-issues)
4. [WebSocket Connection Failures](#4-websocket-connection-failures)
5. [High Memory Usage](#5-high-memory-usage)
6. [Slow Response Times](#6-slow-response-times)
7. [Database Connection Errors](#7-database-connection-errors)
8. [Rate Limiting Issues](#8-rate-limiting-issues)
9. [GraphQL Errors](#9-graphql-errors)
10. [Deployment Issues](#10-deployment-issues)

---

## 1. GIL Deadlock

### Symptoms
- Server stops responding to requests
- CPU usage drops to near zero
- Logs show no new request processing

### Cause
JustAPI uses a dedicated GIL pool to avoid blocking Tokio worker threads. If the GIL pool workers are all busy or the GIL is held too long, requests queue up.

### Solution

**Check GIL pool status:**
```python
import justapi
app = JustAPIApp()
# GIL pool is initialized automatically on first request
```

**Increase GIL pool size (if needed):**
The pool size is auto-detected based on CPU cores. For CPython (GIL enabled), it defaults to 1 worker. For free-threaded Python, it scales with cores.

**Verify no blocking calls in handlers:**
```python
# BAD: Blocks the GIL pool
import time
time.sleep(5)

# GOOD: Use async sleep
import asyncio
await asyncio.sleep(5)
```

**Check for long-running synchronous operations:**
```python
# BAD: Synchronous database call blocks GIL pool
result = db.execute("SELECT * FROM users")

# GOOD: Use async database operations
result = await db.execute_async("SELECT * FROM users")
```

---

## 2. Connection Pool Exhaustion

### Symptoms
- `503 Service Unavailable` responses
- `connection pool exhausted` errors in logs
- Increasing latency under load

### Cause
Too many concurrent database connections or connections not being released.

### Solution

**Check pool configuration:**
```python
from justapi import JustAPIApp, Database

app = JustAPIApp()
db = Database("postgres://user:pass@localhost/db")
app.set_database(db, max_connections=20)  # Default is 10
```

**Monitor active connections:**
```bash
# PostgreSQL
SELECT count(*) FROM pg_stat_activity;

# Check for idle connections
SELECT state, count(*) FROM pg_stat_activity GROUP BY state;
```

**Enable connection pool monitoring:**
```python
# Check pool health
pool_health = await app.db_pool.health_check()
print(f"Pool status: {pool_health}")
```

**Increase pool size (if needed):**
```python
db = Database("postgres://user:pass@localhost/db", max_connections=50)
```

**Reduce connection hold time:**
```python
# BAD: Long transaction holds connection
async with db.transaction():
    result = await db.execute("SELECT * FROM large_table")
    # ... process for 10 seconds ...
    await db.execute("UPDATE ...")

# GOOD: Short transactions
result = await db.execute("SELECT * FROM large_table")
# ... process ...
async with db.transaction():
    await db.execute("UPDATE ...")
```

---

## 3. TLS Certificate Issues

### Symptoms
- `certificate verify failed` errors
- Browser shows certificate warnings
- WebSocket connections fail over TLS

### Cause
Invalid, expired, or misconfigured TLS certificates.

### Solution

**Verify certificate validity:**
```bash
openssl x509 -in cert.pem -noout -dates
openssl x509 -in cert.pem -noout -subject
```

**Check certificate chain:**
```bash
openssl verify -CAfile ca.pem cert.pem
```

**Generate self-signed certificate (development only):**
```bash
openssl req -x509 -newkey rsa:4096 -keyout key.pem -out cert.pem -days 365 -nodes
```

**Verify TLS configuration in JustAPI:**
```python
from justapi import JustAPIApp

app = JustAPIApp()
app.run(
    addr="0.0.0.0:8443",
    tls_cert="cert.pem",
    tls_key="key.pem"
)
```

**Check for common TLS issues:**
1. Certificate expired → renew
2. Wrong private key → ensure key matches certificate
3. Missing intermediate certificates → bundle CA chain
4. hostname mismatch → certificate must cover the domain

---

## 4. WebSocket Connection Failures

### Symptoms
- WebSocket connections immediately close
- `WebSocket connection failed` errors
- Connections establish but no data flows

### Cause
Firewall blocking upgrades, proxy misconfiguration, or handler errors.

### Solution

**Verify WebSocket endpoint is registered:**
```python
from justapi import JustAPIApp

app = JustAPIApp()

@app.websocket("/ws")
async def websocket_handler(ws):
    await ws.accept()
    data = await ws.receive_text()
    await ws.send_text(f"Echo: {data}")
```

**Check proxy configuration (if behind nginx/HAProxy):**
```nginx
# nginx configuration
location /ws {
    proxy_pass http://backend;
    proxy_http_version 1.1;
    proxy_set_header Upgrade $http_upgrade;
    proxy_set_header Connection "upgrade";
    proxy_read_timeout 86400;
}
```

**Test WebSocket connectivity:**
```bash
# Using websocat
websocat ws://localhost:8000/ws

# Using wscat
npx wscat -c ws://localhost:8000/ws
```

**Check for firewall issues:**
```bash
# Test if port is open
telnet localhost 8000

# Check if WebSocket upgrade works
curl -i -N \
  -H "Connection: Upgrade" \
  -H "Upgrade: websocket" \
  -H "Sec-WebSocket-Version: 13" \
  -H "Sec-WebSocket-Key: dGhlIHNhbXBsZSBub25jZQ==" \
  http://localhost:8000/ws
```

---

## 5. High Memory Usage

### Symptoms
- Process RSS grows continuously
- Out of memory kills
- Swap usage increases

### Cause
Memory leaks, large response buffering, or connection leaks.

### Solution

**Monitor memory usage:**
```bash
# Process memory
ps aux | grep justapi

# Detailed memory map
pmap -x <pid>

# Continuous monitoring
watch -n 1 'ps -o pid,rss,vsz -p <pid>'
```

**Check for connection leaks:**
```python
# Ensure connections are properly closed
async with db.connection() as conn:
    result = await conn.execute("SELECT ...")
# Connection automatically returned to pool
```

**Reduce response buffering:**
```python
# BAD: Large response fully buffered
@app.get("/large")
async def large_response():
    return {"data": "x" * 10_000_000}  # 10MB

# GOOD: Stream large responses
@app.get("/stream")
async def stream_response():
    async def generate():
        for i in range(1000):
            yield f"chunk {i}\n"
    return StreamingResponse(generate())
```

**Enable memory profiling:**
```bash
# Using mprof (memory_profiler)
mprof run python app.py

# Analyze results
mprof plot
```

---

## 6. Slow Response Times

### Symptoms
- High p99 latency
- Timeouts under load
- Slow TTFB (Time to First Byte)

### Cause
Slow database queries, blocking operations, or middleware overhead.

### Solution

**Profile request handling:**
```python
import time

@app.middleware("http")
async def timing_middleware(request, call_next):
    start = time.time()
    response = await call_next(request)
    duration = time.time() - start
    print(f"Request took {duration:.3f}s")
    return response
```

**Check database query performance:**
```sql
-- PostgreSQL: Enable slow query logging
ALTER SYSTEM SET log_min_duration_statement = 1000;  -- 1 second
SELECT pg_reload_conf();

-- Check for slow queries
SELECT query, mean_exec_time, calls
FROM pg_stat_statements
ORDER BY mean_exec_time DESC
LIMIT 10;
```

**Verify middleware overhead:**
```python
# Disable middleware temporarily for testing
app = JustAPIApp(middlewares=[])  # No middleware
```

**Check network latency:**
```bash
# Measure round-trip time
ping localhost

# Check DNS resolution time
dig localhost
```

---

## 7. Database Connection Errors

### Symptoms
- `connection refused` errors
- `too many connections` errors
- `connection timeout` errors

### Cause
Database server overload, incorrect connection string, or pool misconfiguration.

### Solution

**Verify connection string:**
```python
from justapi import JustAPIApp, Database

# Correct format
db = Database("postgres://user:password@host:5432/dbname")

# Common mistakes
# Wrong: postgres://user:pass@host/db (missing port)
# Wrong: postgresql://user:pass@host:5432/db (wrong scheme)
```

**Test database connectivity:**
```bash
# PostgreSQL
psql -h localhost -U user -d dbname

# MySQL
mysql -h localhost -u user -p dbname

# SQLite
sqlite3 database.db
```

**Check database server status:**
```bash
# PostgreSQL
sudo systemctl status postgresql

# MySQL
sudo systemctl status mysql
```

**Increase connection timeout:**
```python
db = Database(
    "postgres://user:pass@localhost/db",
    connect_timeout=30,  # seconds
    pool_timeout=30
)
```

---

## 8. Rate Limiting Issues

### Symptoms
- `429 Too Many Requests` responses
- Legitimate requests being blocked
- Inconsistent rate limiting behavior

### Cause
Rate limit configuration too aggressive or Redis connection issues.

### Solution

**Check rate limit configuration:**
```python
from justapi import JustAPIApp

app = JustAPIApp()
# Default: 100 requests per minute
app.set_rate_limit(max_requests=1000, window_seconds=60)
```

**Verify Redis connection (if using distributed rate limiting):**
```bash
redis-cli ping
redis-cli info clients
```

**Check rate limit headers:**
```bash
curl -I http://localhost:8000/api/endpoint
# Look for:
# X-RateLimit-Limit: 100
# X-RateLimit-Remaining: 95
# Retry-After: 30
```

**Adjust rate limits per endpoint:**
```python
@app.get("/api/public", rate_limit={"max_requests": 1000, "window_seconds": 60})
async def public_endpoint():
    return {"message": "public"}

@app.get("/api/sensitive", rate_limit={"max_requests": 10, "window_seconds": 60})
async def sensitive_endpoint():
    return {"message": "sensitive"}
```

---

## 9. GraphQL Errors

### Symptoms
- `Query depth exceeded` errors
- `Query complexity exceeded` errors
- GraphiQL not accessible

### Cause
Query too complex or GraphiQL disabled in production.

### Solution

**Check query depth/complexity:**
```python
from justapi import JustAPIApp

app = JustAPIApp()
# Default limits: depth=10, complexity=200
app.graphql(depth_limit=20, complexity_limit=500)
```

**Enable GraphiQL (development only):**
```bash
# Set environment variable
export JUSTAPI_ENABLE_GRAPHIQL=1
python app.py
```

**Simplify complex queries:**
```graphql
# BAD: Deeply nested query
query {
  users {
    posts {
      comments {
        author {
          name
        }
      }
    }
  }
}

# GOOD: Use fragments and aliases
query {
  users {
    id
    name
    postCount
  }
}
```

---

## 10. Deployment Issues

### Symptoms
- Container fails to start
- Health checks fail
- Pod restarts frequently

### Cause
Misconfigured environment variables, resource limits, or health checks.

### Solution

**Verify environment variables:**
```bash
# Check required variables
echo $DATABASE_URL
echo $REDIS_URL
echo $JUSTAPI_SECRET_KEY
```

**Check container logs:**
```bash
docker logs <container_id>
kubectl logs <pod_name>
```

**Verify health check endpoint:**
```bash
curl http://localhost:8000/health
# Should return: {"status": "healthy"}
```

**Check resource limits:**
```yaml
# Kubernetes resource limits
resources:
  requests:
    memory: "256Mi"
    cpu: "250m"
  limits:
    memory: "512Mi"
    cpu: "500m"
```

**Verify file permissions:**
```bash
# Check secret files
ls -la /run/secrets/
# Should be: -rw------- 1 root root

# Fix permissions if needed
chmod 600 /run/secrets/*
```

---

## Getting Help

If you can't resolve your issue:

1. **Check the logs** for error messages
2. **Search GitHub Issues** for similar problems
3. **Run diagnostics:**
   ```bash
   justapi doctor
   ```
4. **Enable debug logging:**
   ```bash
   export RUST_LOG=debug
   python app.py
   ```
5. **Open a GitHub Issue** with:
   - JustAPI version
   - Python version
   - Error message
   - Steps to reproduce
