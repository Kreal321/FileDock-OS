use std::time::{Duration, Instant};
pub struct Debounce {
    first: Option<Instant>,
    last: Option<Instant>,
    quiet: Duration,
    cap: Duration,
}
impl Default for Debounce {
    fn default() -> Self {
        Self::new(Duration::from_secs(30), Duration::from_secs(60))
    }
}
impl Debounce {
    pub fn new(quiet: Duration, cap: Duration) -> Self {
        Self {
            first: None,
            last: None,
            quiet,
            cap,
        }
    }
    pub fn changed(&mut self, now: Instant) {
        self.first.get_or_insert(now);
        self.last = Some(now);
    }
    pub fn ready(&self, now: Instant) -> bool {
        self.first
            .is_some_and(|first| now.duration_since(first) >= self.cap)
            || self
                .last
                .is_some_and(|last| now.duration_since(last) >= self.quiet)
    }
    pub fn clear(&mut self) {
        self.first = None;
        self.last = None;
    }
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn quiet_and_continuous_changes_are_bounded() {
        let t = Instant::now();
        let mut d = Debounce::default();
        d.changed(t);
        assert!(!d.ready(t + Duration::from_secs(29)));
        assert!(d.ready(t + Duration::from_secs(30)));
        d.changed(t + Duration::from_secs(59));
        assert!(d.ready(t + Duration::from_secs(60)));
        d.clear();
        assert!(!d.ready(t + Duration::from_secs(90)));
    }
}
