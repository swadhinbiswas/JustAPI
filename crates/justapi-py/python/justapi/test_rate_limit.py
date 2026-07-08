import pytest
import asyncio
from justapi import RateLimiter

@pytest.mark.asyncio
async def test_rate_limiter_redis():
    # Requires a running Redis instance on localhost:6379
    # Run with: pytest python/justapi/test_rate_limit.py
    # NOTE: if redis is not running, this might fail with connection error
    try:
        limiter = await RateLimiter.new_redis("redis://127.0.0.1:6379/")
    except Exception as e:
        pytest.skip(f"Redis not available: {e}")

    # capacity=5, replenish_rate=5 (5 tokens per second)
    # The first 5 requests should pass immediately
    for i in range(5):
        res = await limiter.check_limit("test_user_ip", 5, 5, 1)
        assert res.allowed is True, f"Request {i} should be allowed"
        assert res.retry_after_ms == 0

    # 6th request should fail
    res = await limiter.check_limit("test_user_ip", 5, 5, 1)
    assert res.allowed is False, "Request 6 should be rejected"
    assert res.retry_after_ms > 0

    # Wait for 1 token to replenish (0.2 seconds)
    await asyncio.sleep(0.3)
    res = await limiter.check_limit("test_user_ip", 5, 5, 1)
    assert res.allowed is True, "Request should be allowed after wait"
