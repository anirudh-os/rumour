use tokio::time::Instant;

#[derive(Debug)]
pub struct RateLimiter {
    capacity: u64,    // maximum number of tokens
    tokens: u64,      // current tokens available
    refill_rate: u64, // tokens refilled per second
    last_refill: Instant,
}

impl RateLimiter {
    pub fn new(refill_rate: u64, capacity: u64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
        }
    }

    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);
        let refill = elapsed.as_secs_f64() * self.refill_rate as f64;

        if refill >= 1.0 {
            let added = refill as u64;
            self.tokens = (self.tokens + added).min(self.capacity);
            self.last_refill = now;
        }

        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}