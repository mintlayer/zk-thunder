pub mod circuit_breaker;
pub mod config;
pub mod error;
pub mod metrics;
pub mod retry;
pub mod services;
pub mod worker;

use std::sync::Arc;

use tokio::time::Duration;
use zksync_dal::ConnectionPool;

use self::{
    config::DataAvailabilityConfig,
    metrics::DataAvailabilityMetrics,
    worker::{create_data_availability_worker, DataAvailabilityWorker},
};

pub async fn start_data_availability_worker(
    pool: ConnectionPool<zksync_dal::Core>,
) -> Result<(), String> {
    // Load configuration from environment
    let config = DataAvailabilityConfig::from_env()?;
    
    // Initialize metrics
    let metrics = Arc::new(DataAvailabilityMetrics::new());
    
    // Create and run the worker
    let worker = create_data_availability_worker(config, pool, metrics)
        .await
        .map_err(|e| format!("Failed to create data availability worker: {}", e))?;
    
    // Run the worker
    worker.run().await;
    
    // This point should never be reached as run() loops indefinitely
    Ok(())
}
