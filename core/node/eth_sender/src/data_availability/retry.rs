use std::time::Duration;
use tokio::time::sleep;
use super::error::DataAvailabilityError;

pub async fn with_exponential_backoff<F, Fut, T, E>(
    f: F,
    base_delay: Duration,
    max_delay: Duration,
    max_attempts: u32,
    operation_name: &str,
) -> Result<T, E>
where
    F: Fn() -> Fut,
    Fut: std::future::Future<Output = Result<T, E>>,
    E: From<DataAvailabilityError>,
{
    let mut delay = base_delay;
    let mut attempts = 0;

    loop {
        match f().await {
            Ok(result) => return Ok(result),
            Err(e) => {
                attempts += 1;
                
                if attempts >= max_attempts {
                    return Err(DataAvailabilityError::MaxRetriesExceededError(
                        operation_name.into(),
                    ).into());
                }

                tracing::warn!(
                    "{} operation failed (attempt {}): {:?}",
                    operation_name,
                    attempts,
                    e
                );
                
                sleep(delay).await;
                delay = std::cmp::min(delay * 2, max_delay);
            }
        }
    }
} 