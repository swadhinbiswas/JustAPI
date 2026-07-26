use redis::aio::ConnectionManager;
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Clone)]
pub struct RateLimitResult {
    pub allowed: bool,
    pub retry_after_ms: u64,
}

#[derive(Clone)]
pub enum RateLimiter {
    Redis { client: ConnectionManager, script: redis::Script },
    // We can add a local governor fallback later if needed
    // Local(Arc<governor::DefaultDirectRateLimiter>),
}

impl RateLimiter {
    /// Creates a new Redis-backed rate limiter.
    pub async fn new_redis(redis_url: &str) -> anyhow::Result<Self> {
        let client = redis::Client::open(redis_url)?;
        let manager = ConnectionManager::new(client).await?;

        let lua_script = r#"
            local key = KEYS[1]
            local t = tonumber(ARGV[1])
            local tau = tonumber(ARGV[2])
            local now = tonumber(ARGV[3])
            local request = tonumber(ARGV[4])

            local tat = redis.call('GET', key)
            if not tat then
                tat = now
            else
                tat = tonumber(tat)
            end

            local new_tat = math.max(tat, now) + (t * request)
            local allow_at = new_tat - tau

            if allow_at > now then
                -- Rate limited
                local retry_after = math.ceil((allow_at - now) / 1000)
                return {0, retry_after}
            else
                -- Allowed
                local ttl = math.ceil((new_tat - now) / 1000000)
                if ttl > 0 then
                    redis.call('SET', key, new_tat, 'EX', ttl)
                else
                    redis.call('SET', key, new_tat)
                end
                return {1, 0}
            end
        "#;

        Ok(RateLimiter::Redis { client: manager, script: redis::Script::new(lua_script) })
    }

    /// Check if a request is allowed.
    /// `capacity`: maximum burst capacity (e.g., 100)
    /// `replenish_rate`: tokens to add per second (e.g., 10)
    pub async fn check_limit(
        &self,
        key: &str,
        capacity: u64,
        replenish_rate: u64,
        tokens: u64,
    ) -> anyhow::Result<RateLimitResult> {
        match self {
            RateLimiter::Redis { client, script } => {
                let mut conn = client.clone();

                // Interval per token in microseconds
                let t = 1_000_000 / replenish_rate;
                // Burst capacity in microseconds
                let tau = t * capacity;

                let now =
                    SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default().as_micros()
                        as u64;

                let result: Vec<u64> = script
                    .key(key)
                    .arg(t)
                    .arg(tau)
                    .arg(now)
                    .arg(tokens)
                    .invoke_async(&mut conn)
                    .await?;

                if result.len() >= 2 {
                    let allowed = result[0] == 1;
                    let retry_after_ms = result[1];
                    Ok(RateLimitResult { allowed, retry_after_ms })
                } else {
                    Err(anyhow::anyhow!("Invalid response from Redis GCRA script"))
                }
            }
        }
    }
}
