use tokio::time::Instant;

#[derive(Debug)]
pub struct RateLimiter {
    capacity: u64,    // maximum number of tokens
    tokens: u64,      // current tokens available
    refill_rate: u64, // tokens refilled per second
    last_refill: Instant,
    pending: f64,
}

impl RateLimiter {
    pub fn new(refill_rate: u64, capacity: u64) -> Self {
        Self {
            capacity,
            tokens: capacity,
            refill_rate,
            last_refill: Instant::now(),
            pending: 0.0,
        }
    }

    pub fn allow(&mut self) -> bool {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_refill);

        self.pending += elapsed.as_secs_f64() * self.refill_rate as f64;
        self.last_refill = now;

        if self.pending >= 1.0 {
            let added = self.pending as u64;
            self.pending -= added as f64; // keep only the remainder
            self.tokens = (self.tokens + added).min(self.capacity);
        }

        if self.tokens > 0 {
            self.tokens -= 1;
            true
        } else {
            false
        }
    }
}
