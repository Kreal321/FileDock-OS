//! One shared token bucket per configured connection. Sleeps release the executor.
use std::{sync::Arc, time::Duration};
use tokio::{sync::Mutex, time::Instant};
#[derive(Debug)]
pub struct TokenBucket {
    rate: u64,
    next: Mutex<Instant>,
}
impl TokenBucket {
    pub fn new(rate: u64) -> Arc<Self> {
        Arc::new(Self {
            rate: rate.max(1),
            next: Mutex::new(Instant::now()),
        })
    }
    pub async fn acquire(&self, bytes: usize) {
        // Reserve short pieces so an unrelated foreground operation can run between pieces.
        for chunk in (0..bytes).step_by((self.rate / 20).max(1).min(usize::MAX as u64) as usize) {
            let count = (bytes - chunk).min((self.rate / 20).max(1) as usize);
            let deadline = {
                let mut next = self.next.lock().await;
                let start = (*next).max(Instant::now());
                *next = start + Duration::from_secs_f64(count as f64 / self.rate as f64);
                *next
            };
            tokio::time::sleep_until(deadline).await;
        }
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[tokio::test]
    async fn concurrent_consumers_share_the_limit() {
        let bucket = TokenBucket::new(1000);
        let start = Instant::now();
        tokio::join!(bucket.acquire(500), bucket.acquire(500));
        assert!(start.elapsed() >= Duration::from_secs(1));
    }
}
