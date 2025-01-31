use std::time::{Duration, Instant};

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    failures: u32,
    last_failure: Option<Instant>,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            failures: 0,
            last_failure: None,
        }
    }

    pub fn is_open(&self) -> bool {
        if let Some(last_failure) = self.last_failure {
            if last_failure.elapsed() >= self.reset_timeout {
                return false;
            }
        }
        self.failures >= self.failure_threshold
    }

    pub fn record_failure(&mut self) -> bool {
        self.failures += 1;
        self.last_failure = Some(Instant::now());
        self.failures >= self.failure_threshold
    }
}
