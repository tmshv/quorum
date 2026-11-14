//! Token bucket rate limiter with a permanent strike counter.

use std::time::Instant;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Refuse,
    Disconnect,
}

/// Token bucket: starts full, refills continuously, caps at `burst`.
/// Strikes (refusals) are never reset.
#[derive(Debug)]
pub struct Limiter {
    rate: f64,
    burst: f64,
    max_strikes: u32,
    tokens: f64,
    last: Option<Instant>,
    strikes: u32,
}

impl Limiter {
    pub fn new(rate_per_sec: f64, burst: f64, max_strikes: u32) -> Limiter {
        Limiter {
            rate: rate_per_sec,
            burst,
            max_strikes,
            tokens: burst,
            last: None,
            strikes: 0,
        }
    }

    pub fn check(&mut self) -> Verdict {
        self.check_at(Instant::now())
    }

    pub fn check_at(&mut self, now: Instant) -> Verdict {
        if let Some(last) = self.last {
            let elapsed = now.saturating_duration_since(last).as_secs_f64();
            self.tokens = (self.tokens + elapsed * self.rate).min(self.burst);
        }
        self.last = Some(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Verdict::Allow;
        }
        self.strikes += 1;
        if self.strikes > self.max_strikes {
            Verdict::Disconnect
        } else {
            Verdict::Refuse
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::limits::{MAX_STRIKES, RATE_BURST, RATE_PER_SEC};
    use std::time::Duration;

    fn limiter() -> Limiter {
        Limiter::new(RATE_PER_SEC, RATE_BURST, MAX_STRIKES)
    }

    fn drain(l: &mut Limiter, t: Instant) {
        for _ in 0..60 {
            assert_eq!(l.check_at(t), Verdict::Allow);
        }
    }

    #[test]
    fn burst_then_refuse() {
        let t = Instant::now();
        let mut l = limiter();
        drain(&mut l, t);
        assert_eq!(l.check_at(t), Verdict::Refuse);
    }

    #[test]
    fn refills_over_time() {
        let t = Instant::now();
        let mut l = limiter();
        drain(&mut l, t);
        let t2 = t + Duration::from_millis(100);
        for _ in 0..3 {
            assert_eq!(l.check_at(t2), Verdict::Allow);
        }
        assert_eq!(l.check_at(t2), Verdict::Refuse);
    }

    #[test]
    fn disconnects_after_strikes() {
        let t = Instant::now();
        let mut l = limiter();
        drain(&mut l, t);
        for _ in 0..100 {
            assert_eq!(l.check_at(t), Verdict::Refuse);
        }
        assert_eq!(l.check_at(t), Verdict::Disconnect);
    }

    #[test]
    fn tokens_cap_at_burst() {
        let t = Instant::now();
        let mut l = limiter();
        drain(&mut l, t);
        let t2 = t + Duration::from_secs(10);
        for _ in 0..60 {
            assert_eq!(l.check_at(t2), Verdict::Allow);
        }
        assert_eq!(l.check_at(t2), Verdict::Refuse);
    }
}
