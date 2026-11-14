//! Token bucket rate limiter with a decaying strike counter.

use std::time::{Duration, Instant};

use crate::limits::{MAX_STRIKES, RATE_BURST, RATE_PER_SEC, STRIKE_RESET};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verdict {
    Allow,
    Refuse,
    Disconnect,
}

/// A connection's message budget.
#[derive(Debug, Clone, Copy)]
pub struct Budget {
    pub per_sec: f64,
    pub burst: f64,
    /// Refusals tolerated within one strike window; one more disconnects.
    pub max_strikes: u32,
    /// Strikes reset after this long without a refusal.
    pub strike_reset: Duration,
}

impl Default for Budget {
    fn default() -> Self {
        Budget {
            per_sec: RATE_PER_SEC,
            burst: RATE_BURST,
            max_strikes: MAX_STRIKES,
            strike_reset: STRIKE_RESET,
        }
    }
}

/// Token bucket: starts full, refills continuously, caps at `burst`.
#[derive(Debug)]
pub struct Limiter {
    budget: Budget,
    tokens: f64,
    last: Option<Instant>,
    strikes: u32,
    last_refusal: Option<Instant>,
}

impl Limiter {
    pub fn new(budget: Budget) -> Limiter {
        Limiter {
            budget,
            tokens: budget.burst,
            last: None,
            strikes: 0,
            last_refusal: None,
        }
    }

    pub fn check(&mut self) -> Verdict {
        self.check_at(Instant::now())
    }

    pub fn check_at(&mut self, now: Instant) -> Verdict {
        if let Some(last) = self.last {
            let elapsed = now.saturating_duration_since(last).as_secs_f64();
            self.tokens = (self.tokens + elapsed * self.budget.per_sec).min(self.budget.burst);
        }
        self.last = Some(now);
        if self.tokens >= 1.0 {
            self.tokens -= 1.0;
            return Verdict::Allow;
        }
        if let Some(prev) = self.last_refusal
            && now.saturating_duration_since(prev) >= self.budget.strike_reset
        {
            self.strikes = 0;
        }
        self.last_refusal = Some(now);
        self.strikes += 1;
        if self.strikes > self.budget.max_strikes {
            Verdict::Disconnect
        } else {
            Verdict::Refuse
        }
    }

    /// Returns the token taken by the last allowed message (it cost nothing).
    pub fn refund(&mut self) {
        self.tokens = (self.tokens + 1.0).min(self.budget.burst);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limiter() -> Limiter {
        Limiter::new(Budget::default())
    }

    fn drain(l: &mut Limiter, t: Instant) {
        for _ in 0..360 {
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
        for _ in 0..12 {
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
        drain(&mut l, t2);
        assert_eq!(l.check_at(t2), Verdict::Refuse);
    }

    /// No refill, so only strikes and refunds change the outcome.
    fn frozen(max_strikes: u32) -> Limiter {
        Limiter::new(Budget {
            per_sec: 0.0,
            burst: 1.0,
            max_strikes,
            strike_reset: Duration::from_secs(10),
        })
    }

    #[test]
    fn refund_returns_the_token() {
        let t = Instant::now();
        let mut l = frozen(5);
        assert_eq!(l.check_at(t), Verdict::Allow);
        l.refund();
        assert_eq!(l.check_at(t), Verdict::Allow);
        assert_eq!(l.check_at(t), Verdict::Refuse);
    }

    #[test]
    fn refund_caps_at_burst() {
        let t = Instant::now();
        let mut l = frozen(5);
        l.refund();
        l.refund();
        assert_eq!(l.check_at(t), Verdict::Allow);
        assert_eq!(l.check_at(t), Verdict::Refuse);
    }

    #[test]
    fn strikes_reset_after_a_quiet_window() {
        let t = Instant::now();
        let mut l = frozen(3);
        assert_eq!(l.check_at(t), Verdict::Allow);
        for _ in 0..3 {
            assert_eq!(l.check_at(t), Verdict::Refuse);
        }
        // Ten seconds without a refusal: the count starts over.
        let t2 = t + Duration::from_secs(10);
        for _ in 0..3 {
            assert_eq!(l.check_at(t2), Verdict::Refuse);
        }
        assert_eq!(l.check_at(t2), Verdict::Disconnect);
    }

    #[test]
    fn strikes_accumulate_while_refusals_continue() {
        let t = Instant::now();
        let mut l = frozen(3);
        assert_eq!(l.check_at(t), Verdict::Allow);
        // Each refusal comes before the window since the previous one ends.
        for i in 0..3 {
            let at = t + Duration::from_secs(9 * i);
            assert_eq!(l.check_at(at), Verdict::Refuse);
        }
        assert_eq!(l.check_at(t + Duration::from_secs(27)), Verdict::Disconnect);
    }
}
