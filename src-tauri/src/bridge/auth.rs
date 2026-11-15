//! 桥接鉴权原语：常量时间令牌比较、Origin 前缀校验与令牌桶限流。
//!
//! 契约见 docs/BACKEND_STRUCTURE.md §5.3。这里不读取配置，调用方传入当前值，
//! 因此令牌轮换后下一次请求立即使用新令牌，无需重启服务。

use std::sync::Mutex;
use std::time::Instant;

/// 允许的 Origin 前缀；只做前缀校验，不声称校验扩展 ID。
pub const ALLOWED_ORIGIN_PREFIX: &str = "moz-extension://";

/// 稳定速率：20 次/秒。
pub const REFILL_PER_SECOND: f64 = 20.0;
/// 突发容量：40 次。
pub const BURST_CAPACITY: f64 = 40.0;

/// 常量时间比较令牌，长度差异也计入结果，不提前返回。
pub fn tokens_match(expected: &str, provided: &str) -> bool {
    if expected.is_empty() {
        return false;
    }
    let expected = expected.as_bytes();
    let provided = provided.as_bytes();
    let mut difference = (expected.len() ^ provided.len()) as u64;
    let longest = expected.len().max(provided.len());
    for index in 0..longest {
        let left = expected.get(index).copied().unwrap_or_default();
        let right = provided.get(index).copied().unwrap_or_default();
        difference |= u64::from(left ^ right);
    }
    difference == 0
}

/// 缺失 Origin（curl 等调试工具）放行，仍需令牌；存在时必须是扩展来源。
pub fn origin_allowed(origin: Option<&str>) -> bool {
    match origin {
        None => true,
        Some(value) => value
            .get(..ALLOWED_ORIGIN_PREFIX.len())
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case(ALLOWED_ORIGIN_PREFIX)),
    }
}

/// 令牌桶限流：按时间线性补充令牌，桶空时拒绝。
pub struct RateLimiter {
    bucket: Mutex<Bucket>,
    refill_per_second: f64,
    capacity: f64,
}

struct Bucket {
    tokens: f64,
    last: Instant,
}

impl RateLimiter {
    pub fn new(refill_per_second: f64, capacity: f64) -> Self {
        Self {
            bucket: Mutex::new(Bucket {
                tokens: capacity,
                last: Instant::now(),
            }),
            refill_per_second,
            capacity,
        }
    }

    /// 契约默认值：20 次/秒、突发 40。
    pub fn default_limits() -> Self {
        Self::new(REFILL_PER_SECOND, BURST_CAPACITY)
    }

    /// 尝试消耗一个令牌；返回 true 表示本次请求放行。
    pub fn check(&self) -> bool {
        self.check_at(Instant::now())
    }

    /// 可注入时钟的版本，便于测试补充速率。
    pub fn check_at(&self, now: Instant) -> bool {
        let mut bucket = self
            .bucket
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner());
        let elapsed = now.saturating_duration_since(bucket.last).as_secs_f64();
        bucket.tokens = (bucket.tokens + elapsed * self.refill_per_second).min(self.capacity);
        bucket.last = now;
        if bucket.tokens >= 1.0 {
            bucket.tokens -= 1.0;
            true
        } else {
            false
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn tokens_match_requires_exact_value() {
        assert!(tokens_match("abc123", "abc123"));
        assert!(!tokens_match("abc123", "abc124"));
        assert!(!tokens_match("abc123", "abc12"));
        assert!(!tokens_match("abc123", "abc1234"));
        assert!(!tokens_match("", ""));
    }

    #[test]
    fn origin_must_be_extension_prefix() {
        assert!(origin_allowed(None));
        assert!(origin_allowed(Some("moz-extension://1234-abcd")));
        assert!(origin_allowed(Some("MOZ-EXTENSION://1234")));
        assert!(!origin_allowed(Some("https://example.com")));
        assert!(!origin_allowed(Some("moz-extension-evil://1")));
    }

    #[test]
    fn limiter_allows_burst_then_refills() {
        let limiter = RateLimiter::new(20.0, 40.0);
        let start = Instant::now();
        for _ in 0..40 {
            assert!(limiter.check_at(start));
        }
        assert!(!limiter.check_at(start));
        assert!(limiter.check_at(start + Duration::from_millis(50)));
        assert!(!limiter.check_at(start + Duration::from_millis(50)));
    }

    #[test]
    fn limiter_caps_at_capacity() {
        let limiter = RateLimiter::new(20.0, 2.0);
        let start = Instant::now();
        assert!(limiter.check_at(start));
        assert!(limiter.check_at(start));
        assert!(!limiter.check_at(start));
        // 长时间空闲后也不会超过容量。
        assert!(limiter.check_at(start + Duration::from_secs(60)));
        assert!(limiter.check_at(start + Duration::from_secs(60)));
        assert!(!limiter.check_at(start + Duration::from_secs(60)));
    }
}
