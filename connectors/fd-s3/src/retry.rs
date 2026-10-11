use std::time::Duration;

/// Exponential backoff with jitter. Conditional PUTs are retried only after a re-read.
pub fn backoff(attempt: u32) -> Duration {
    let base_ms = 100u64.saturating_mul(1u64 << attempt.min(5));
    let jitter = attempt as u64 * 17 % 50;
    Duration::from_millis(base_ms + jitter)
}

pub fn should_retry(status: Option<u16>, attempts: u32) -> bool {
    attempts < 5
        && match status {
            None => true,
            Some(code) => code == 429 || (500..600).contains(&code),
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn five_attempts_then_stop() {
        assert!(should_retry(Some(503), 0));
        assert!(!should_retry(Some(503), 5));
        assert!(!should_retry(Some(404), 0));
        assert!(backoff(0) < backoff(3));
    }
}
