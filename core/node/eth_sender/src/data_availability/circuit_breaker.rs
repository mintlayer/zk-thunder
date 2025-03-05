use std::time::{Duration, Instant};

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CircuitBreakerState {
    Closed,
    Open,
    HalfOpen,
}

#[derive(Debug)]
pub struct CircuitBreaker {
    failure_threshold: u32,
    reset_timeout: Duration,
    half_open_timeout: Duration,
    failure_count: u32,
    last_failure_time: Option<Instant>,
    state: CircuitBreakerState,
}

impl CircuitBreaker {
    pub fn new(failure_threshold: u32, reset_timeout: Duration) -> Self {
        Self {
            failure_threshold,
            reset_timeout,
            half_open_timeout: reset_timeout / 2,
            failure_count: 0,
            last_failure_time: None,
            state: CircuitBreakerState::Closed,
        }
    }

    pub fn with_half_open_timeout(mut self, half_open_timeout: Duration) -> Self {
        self.half_open_timeout = half_open_timeout;
        self
    }

    pub fn is_open(&mut self) -> bool {
        self.check_state();
        self.state != CircuitBreakerState::Closed
    }

    pub fn record_failure(&mut self) -> bool {
        self.failure_count += 1;
        self.last_failure_time = Some(Instant::now());
        
        if self.state == CircuitBreakerState::HalfOpen || 
           (self.state == CircuitBreakerState::Closed && self.failure_count >= self.failure_threshold) {
            self.state = CircuitBreakerState::Open;
            true
        } else {
            false
        }
    }

    pub fn record_success(&mut self) {
        if self.state == CircuitBreakerState::HalfOpen {
            self.state = CircuitBreakerState::Closed;
            self.failure_count = 0;
            self.last_failure_time = None;
        }
    }

    fn check_state(&mut self) {
        if self.state == CircuitBreakerState::Open {
            if let Some(last_failure) = self.last_failure_time {
                if last_failure.elapsed() >= self.reset_timeout {
                    self.state = CircuitBreakerState::HalfOpen;
                    self.failure_count = 0;
                }
            }
        } else if self.state == CircuitBreakerState::HalfOpen {
            if let Some(last_failure) = self.last_failure_time {
                if last_failure.elapsed() >= self.half_open_timeout {
                    // Allow a test request to go through
                    self.state = CircuitBreakerState::HalfOpen;
                }
            }
        }
    }

    pub fn get_state(&mut self) -> CircuitBreakerState {
        self.check_state();
        self.state
    }
}
