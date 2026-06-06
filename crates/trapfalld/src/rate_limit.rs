//! Token bucket rate limiter — per-project.
//!
//! Lightweight in-memory rate limiting. Each project gets a bucket
//! that refills at a configurable rate. No external deps needed.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Instant;

/// Token bucket for a single project.
#[derive(Debug)]
struct Bucket {
    tokens: f64,
    max_tokens: f64,
    refill_per_sec: f64,
    last_refill: Instant,
}

impl Bucket {
    fn new(max_tokens: f64, refill_per_sec: f64) -> Self {
        Self { tokens: max_tokens, max_tokens, refill_per_sec, last_refill: Instant::now() }
    }

    fn try_consume(&mut self, cost: f64) -> bool {
        self.refill();
        if self.tokens >= cost {
            self.tokens -= cost;
            true
        } else {
            false
        }
    }

    fn refill(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill).as_secs_f64();
        self.tokens = (self.tokens + elapsed * self.refill_per_sec).min(self.max_tokens);
        self.last_refill = now;
    }
}

/// Per-project rate limiter.
#[derive(Clone)]
pub struct RateLimiter {
    buckets: Arc<Mutex<HashMap<String, Bucket>>>,
    max_tokens: f64,
    refill_per_sec: f64,
}

impl RateLimiter {
    /// Create a new rate limiter.
    ///
    /// - `max_tokens`: maximum burst size (e.g., 100)
    /// - `refill_per_sec`: tokens added per second (e.g., 10)
    pub fn new(max_tokens: f64, refill_per_sec: f64) -> Self {
        Self { buckets: Arc::new(Mutex::new(HashMap::new())), max_tokens, refill_per_sec }
    }

    /// Try to consume `cost` tokens for the given project.
    /// Returns true if allowed, false if rate limited.
    pub fn try_consume(&self, project_id: &str, cost: f64) -> bool {
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets
            .entry(project_id.to_string())
            .or_insert_with(|| Bucket::new(self.max_tokens, self.refill_per_sec));
        bucket.try_consume(cost)
    }

    /// Get current token count for a project (for metrics).
    #[allow(dead_code)]
    pub fn available_tokens(&self, project_id: &str) -> f64 {
        let mut buckets = self.buckets.lock().unwrap();
        let bucket = buckets
            .entry(project_id.to_string())
            .or_insert_with(|| Bucket::new(self.max_tokens, self.refill_per_sec));
        bucket.refill();
        bucket.tokens
    }
}

/// Default rate limiter: 100 burst, 10/sec refill.
impl Default for RateLimiter {
    fn default() -> Self {
        Self::new(100.0, 10.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allows_within_limit() {
        let limiter = RateLimiter::new(10.0, 1.0);
        assert!(limiter.try_consume("proj1", 1.0));
        assert!(limiter.try_consume("proj1", 1.0));
    }

    #[test]
    fn blocks_over_limit() {
        let limiter = RateLimiter::new(3.0, 0.0); // no refill
        assert!(limiter.try_consume("proj1", 1.0));
        assert!(limiter.try_consume("proj1", 1.0));
        assert!(limiter.try_consume("proj1", 1.0));
        assert!(!limiter.try_consume("proj1", 1.0)); // blocked
    }

    #[test]
    fn independent_projects() {
        let limiter = RateLimiter::new(1.0, 0.0);
        assert!(limiter.try_consume("proj1", 1.0));
        assert!(limiter.try_consume("proj2", 1.0)); // different project
        assert!(!limiter.try_consume("proj1", 1.0)); // proj1 blocked
    }

    #[test]
    fn default_limiter() {
        let limiter = RateLimiter::default();
        // Should allow 100 burst
        for _ in 0..100 {
            assert!(limiter.try_consume("proj1", 1.0));
        }
        assert!(!limiter.try_consume("proj1", 1.0));
    }

    #[test]
    fn available_tokens_tracking() {
        let limiter = RateLimiter::new(10.0, 0.0);
        limiter.try_consume("proj1", 3.0);
        assert_eq!(limiter.available_tokens("proj1"), 7.0);
    }
}
