#[derive(Debug, thiserror::Error)]
pub enum DataAvailabilityError {
    #[error("IPFS error: {0}")]
    IPFSError(String),
    #[error("Mintlayer error: {0}")]
    MintlayerError(String),
    #[error("Database error: {0}")]
    DatabaseError(String),
    #[error("Maximum retries exceeded error: {0}")]
    MaxRetriesExceededError(String),
    #[error("Circuit breaker open error: {0}")]
    CircuitBreakerOpenError(String),
    #[error("Configuration error: {0}")]
    ConfigError(String),
}
