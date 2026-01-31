#![forbid(unsafe_code)]

use std::time::{Duration, Instant};

#[async_trait::async_trait]
pub trait Clock: Send + Sync {
    fn now(&self) -> Instant;
    async fn sleep(&self, duration: Duration);
}

#[derive(Debug, Default)]
pub struct SystemClock;

#[async_trait::async_trait]
impl Clock for SystemClock {
    fn now(&self) -> Instant {
        Instant::now()
    }

    async fn sleep(&self, duration: Duration) {
        tokio::time::sleep(duration).await;
    }
}
