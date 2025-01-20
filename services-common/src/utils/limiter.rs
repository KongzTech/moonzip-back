use std::{num::NonZeroU32, sync::Arc, time::Duration};

use anyhow::Context;
use governor::{clock::DefaultClock, state::InMemoryState, Jitter, Quota, RateLimiter};
use serde::Deserialize;

pub type InMemoryLimiter = RateLimiter<governor::state::NotKeyed, InMemoryState, DefaultClock>;
pub type SharedRateLimiter = Arc<InMemoryLimiter>;

#[derive(Deserialize, Debug, Clone, serde_derive_default::Default)]
pub struct RateLimitConfig {
    #[serde(default = "default_burst")]
    pub burst: NonZeroU32,
    #[serde(default)]
    pub jitter: JitterConfig,
}

#[derive(Deserialize, Debug, Clone, serde_derive_default::Default)]
pub struct JitterConfig {
    #[serde(default = "default_jitter_min")]
    min: Duration,
    #[serde(default = "default_jitter_interval")]
    interval: Duration,
}

pub fn default_jitter_min() -> Duration {
    Duration::from_millis(100)
}

pub fn default_jitter_interval() -> Duration {
    Duration::from_millis(200)
}

pub fn default_burst() -> NonZeroU32 {
    NonZeroU32::new(1).unwrap()
}

impl RateLimitConfig {
    pub fn limiter(&self) -> Limiter {
        Limiter {
            inner: RateLimiter::direct(Quota::per_second(self.burst)),
            jitter: Jitter::new(self.jitter.min, self.jitter.interval),
        }
    }
}

pub struct Limiter {
    inner: InMemoryLimiter,
    jitter: Jitter,
}

impl Limiter {
    pub async fn until_ready(&self) {
        self.inner.until_ready_with_jitter(self.jitter).await;
    }

    pub async fn until_n_ready(&self, amount: NonZeroU32) -> anyhow::Result<()> {
        self.inner
            .until_n_ready_with_jitter(amount, self.jitter)
            .await
            .context("doesn't fit into limiter")?;
        Ok(())
    }
}

pub struct LimiterGuard<T> {
    inner: T,
    limiter: Limiter,
}

impl<T> LimiterGuard<T> {
    pub fn new(val: T, limiter: Limiter) -> Self {
        Self {
            inner: val,
            limiter,
        }
    }

    pub async fn use_single(&self) -> &T {
        self.limiter.until_ready().await;
        &self.inner
    }

    pub async fn use_multiple(&self, n: NonZeroU32) -> &T {
        self.limiter.until_n_ready(n).await.unwrap();
        &self.inner
    }
}
