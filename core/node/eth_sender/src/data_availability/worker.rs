use super::{
    circuit_breaker::{CircuitBreaker, CircuitBreakerState},
    config::{DataAvailabilityConfig, WorkerConfig},
    error::DataAvailabilityError,
    metrics::DataAvailabilityMetrics,
    retry::with_exponential_backoff,
    services::{IPFSService, MintlayerService},
};
use base64::Engine;
use s3::Bucket;
use std::{io::Cursor, sync::Arc, time::Instant};
use tokio::sync::Mutex;
use tokio::time::Duration;
use uuid::Uuid;
use zksync_dal::{
    data_availability_dal::{OperationStatus, PendingIpfsOperation, PendingMintlayerBatch},
    Connection, ConnectionPool, Core, CoreDal,
};

// Helper function for transaction management
async fn with_transaction<F, T>(
    conn: &mut Connection<'_, Core>,
    f: F,
) -> Result<T, DataAvailabilityError>
where
    F: FnOnce(&mut Connection<'_, Core>) -> Result<T, DataAvailabilityError> + Send,
    T: Send,
{
    let mut tx = conn
        .start_transaction()
        .await
        .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

    match f(&mut tx).await {
        Ok(result) => {
            tx.commit()
                .await
                .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
            Ok(result)
        }
        Err(e) => {
            // Attempt to roll back, but don't propagate rollback errors
            let _ = tx.rollback().await;
            Err(e)
        }
    }
}

#[derive(Debug)]
pub struct DataAvailabilityWorker<I: IPFSService, M: MintlayerService> {
    config: WorkerConfig,
    pool: ConnectionPool<Core>,
    metrics: Arc<DataAvailabilityMetrics>,
    ipfs_service: Arc<I>,
    mintlayer_service: Arc<M>,
    ipfs_circuit_breaker: Mutex<CircuitBreaker>,
    mintlayer_circuit_breaker: Mutex<CircuitBreaker>,
}

impl<I: IPFSService, M: MintlayerService> DataAvailabilityWorker<I, M> {
    pub fn new(
        config: WorkerConfig,
        pool: ConnectionPool<Core>,
        metrics: Arc<DataAvailabilityMetrics>,
        ipfs_service: Arc<I>,
        mintlayer_service: Arc<M>,
    ) -> Self {
        Self {
            ipfs_circuit_breaker: Mutex::new(
                CircuitBreaker::new(5, Duration::from_secs(300))
                    .with_half_open_timeout(Duration::from_secs(30)),
            ),
            mintlayer_circuit_breaker: Mutex::new(
                CircuitBreaker::new(5, Duration::from_secs(300))
                    .with_half_open_timeout(Duration::from_secs(30)),
            ),
            config,
            pool,
            metrics,
            ipfs_service,
            mintlayer_service,
        }
    }

    async fn process_ipfs_operation(
        &self,
        op: &mut PendingIpfsOperation,
    ) -> Result<(), DataAvailabilityError> {
        let mut circuit_breaker = self.ipfs_circuit_breaker.lock().await;
        if circuit_breaker.is_open() {
            if circuit_breaker.get_state() == CircuitBreakerState::HalfOpen {
                // Allow one test request through in half-open state
                tracing::info!("Circuit breaker in half-open state, allowing test request");
            } else {
                self.metrics.circuit_breaker_trips.inc();
                return Err(DataAvailabilityError::CircuitBreakerOpenError(
                    "IPFS".into(),
                ));
            }
        }
        drop(circuit_breaker); // Release the lock before the long-running operation

        let start = Instant::now();
        let result = self.upload_to_ipfs_with_backoff(op).await;
        let duration = start.elapsed();
        self.metrics.ipfs_operation_duration.observe(duration);

        match result {
            Ok(hash) => {
                self.metrics.ipfs_success.inc();
                self.ipfs_circuit_breaker.lock().await.record_success();

                let mut conn = self
                    .pool
                    .connection_tagged("data_availability_worker")
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                with_transaction(&mut conn, |tx| async move {
                    op.status = OperationStatus::Completed;
                    op.ipfs_hash = Some(hash.clone());
                    tx.data_availability_dal()
                        .update_ipfs_operations(op)
                        .await
                        .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                    if op.requires_mintlayer {
                        self.queue_mintlayer_batch(tx, hash).await?;
                    }
                    
                    Ok(())
                })
                .await
            }
            Err(e) => {
                self.metrics.ipfs_errors.inc();
                if self.ipfs_circuit_breaker.lock().await.record_failure() {
                    tracing::error!("Circuit breaker opened for IPFS operations");
                }
                Err(e)
            }
        }
    }

    async fn upload_to_ipfs_with_backoff(
        &self,
        op: &mut PendingIpfsOperation,
    ) -> Result<String, DataAvailabilityError> {
        let data = op.data.clone();
        
        with_exponential_backoff(
            || async {
                op.attempts += 1;
                self.ipfs_service.upload(&data).await
            },
            self.config.ipfs_retry_base_delay,
            self.config.ipfs_retry_max_delay,
            self.config.ipfs_max_attempts - op.attempts,
            "IPFS",
        )
        .await
    }

    async fn process_mintlayer_batch(
        &self,
        batch: &mut PendingMintlayerBatch,
    ) -> Result<(), DataAvailabilityError> {
        let mut circuit_breaker = self.mintlayer_circuit_breaker.lock().await;
        if circuit_breaker.is_open() {
            if circuit_breaker.get_state() == CircuitBreakerState::HalfOpen {
                // Allow one test request through in half-open state
                tracing::info!("Circuit breaker in half-open state, allowing test request");
            } else {
                self.metrics.circuit_breaker_trips.inc();
                return Err(DataAvailabilityError::CircuitBreakerOpenError(
                    "Mintlayer".into(),
                ));
            }
        }
        drop(circuit_breaker); // Release the lock before the long-running operation

        let start = Instant::now();
        let result = self.submit_to_mintlayer_with_backoff(batch).await;
        let duration = start.elapsed();
        self.metrics.mintlayer_operation_duration.observe(duration);

        match result {
            Ok(tx_hash) => {
                self.metrics.mintlayer_success.inc();
                self.mintlayer_circuit_breaker.lock().await.record_success();

                let mut conn = self
                    .pool
                    .connection_tagged("data_availability_worker")
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                with_transaction(&mut conn, |tx| async move {
                    batch.status = OperationStatus::Completed;
                    batch.tx_hash = Some(tx_hash);

                    tx.data_availability_dal()
                        .update_mintlayer_batch(batch)
                        .await
                        .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
                    
                    Ok(())
                })
                .await
            }
            Err(e) => {
                self.metrics.mintlayer_errors.inc();
                if self.mintlayer_circuit_breaker.lock().await.record_failure() {
                    tracing::error!("Circuit breaker opened for Mintlayer operations");
                }
                Err(e)
            }
        }
    }

    async fn submit_to_mintlayer_with_backoff(
        &self,
        batch: &mut PendingMintlayerBatch,
    ) -> Result<String, DataAvailabilityError> {
        let ipfs_hashes = batch.ipfs_hashes.clone();
        
        with_exponential_backoff(
            || async {
                batch.attempts += 1;
                self.mintlayer_service.submit_hashes(&ipfs_hashes).await
            },
            self.config.mintlayer_retry_base,
            self.config.mintlayer_retry_max_delay,
            self.config.mintlayer_max_attempts - batch.attempts,
            "Mintlayer",
        )
        .await
    }

    pub async fn run(self) {
        // Initialize Mintlayer wallet
        if let Err(e) = self.mintlayer_service.initialize_wallet().await {
            tracing::error!("Failed to initialize Mintlayer wallet: {}", e);
            // Continue anyway, as the wallet might already be initialized
        }

        let self_arc = Arc::new(self);

        let cleanup_task = {
            let worker = Arc::clone(&self_arc);
            tokio::spawn(async move { worker.run_cleanup_routine().await })
        };

        let ipfs_task = {
            let worker = Arc::clone(&self_arc);
            tokio::spawn(async move { worker.run_ipfs_worker().await })
        };

        let mintlayer_task = {
            let worker = Arc::clone(&self_arc);
            tokio::spawn(async move { worker.run_mintlayer_worker().await })
        };

        tokio::select! {
            result = cleanup_task => {
                if let Err(e) = result {
                    tracing::error!("Cleanup task failed: {}", e);
                }
            }

            result = ipfs_task => {
                if let Err(e) = result {
                    tracing::error!("IPFS task failed {}", e);
                }
            }

            result = mintlayer_task => {
                if let Err(e) = result {
                    tracing::error!("Mintlayer task failed: {}", e);
                }
            }
        }
    }

    async fn run_cleanup_routine(&self) {
        loop {
            if let Ok(mut conn) = self
                .pool
                .connection_tagged("data_availability_worker")
                .await
                .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))
            {
                if let Err(e) = conn
                    .data_availability_dal()
                    .cleanup_old_operations(self.config.cleanup_days_threshold)
                    .await
                {
                    tracing::error!("Cleanup routine failed: {}", e);
                }
            };
            tokio::time::sleep(self.config.cleanup_interval).await;
        }
    }

    async fn run_ipfs_worker(&self) {
        loop {
            if let Ok(mut conn) = self
                .pool
                .connection_tagged("data_availability_worker")
                .await
            {
                match conn
                    .data_availability_dal()
                    .get_pending_ipfs_operations()
                    .await
                {
                    Ok(operations) => {
                        self.metrics.ipfs_queue_size.set(operations.len());
                        for mut op in operations {
                            if let Err(e) = self.process_ipfs_operation(&mut op).await {
                                tracing::error!(
                                    "Failed to process IPFS operation {}: {}",
                                    op.id,
                                    e
                                );
                            } else {
                                tracing::info!("IPFS operation with id {} processed", op.id);
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to get pending IPFS operations: {}", e);
                    }
                }
            };
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn run_mintlayer_worker(&self) {
        loop {
            if let Ok(mut conn) = self
                .pool
                .connection_tagged("data_availability_worker")
                .await
            {
                match conn
                    .data_availability_dal()
                    .get_pending_mintlayer_batches()
                    .await
                {
                    Ok(batches) => {
                        self.metrics.mintlayer_queue_size.set(batches.len());
                        for mut batch in batches {
                            if let Err(e) = self.process_mintlayer_batch(&mut batch).await {
                                tracing::error!(
                                    "Failed to process Mintlayer batch {}: {}",
                                    batch.id,
                                    e
                                );
                            }
                        }
                    }
                    Err(e) => {
                        tracing::error!("Failed to get pending Mintlayer batches: {}", e);
                    }
                }
            };
            tokio::time::sleep(Duration::from_secs(1)).await;
        }
    }

    async fn queue_mintlayer_batch(
        &self,
        mut tx: Connection<'_, Core>,
        hash: String,
    ) -> Result<(), DataAvailabilityError> {
        // Get all pending batches
        let mut batches = tx
            .data_availability_dal()
            .get_pending_mintlayer_batches()
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        // Find a batch that's not full yet, or create a new one
        let batch = batches
            .iter_mut()
            .find(|b| {
                b.status == OperationStatus::Pending && b.ipfs_hashes.len() < self.config.batch_size
            })
            .unwrap_or_else(|| {
                // Create a new batch if none exists or all are full
                let mut new_batch = PendingMintlayerBatch::new();
                batches.push(new_batch);
                batches.last_mut().unwrap()
            });

        // Add the hash to the batch
        batch.ipfs_hashes.push(hash);

        // Mark as ready if the batch is full
        if batch.ipfs_hashes.len() >= self.config.batch_size {
            batch.status = OperationStatus::Ready;
        }

        // Update the batch in the database
        tx.data_availability_dal()
            .update_mintlayer_batch(batch)
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        Ok(())
    }

    async fn upload_to_ipfs(
        &self,
        bucket: &Bucket,
        doc_name: &str,
        mut contents: Cursor<Vec<u8>>,
    ) -> Result<String, DataAvailabilityError> {
        match bucket.put_object_stream(&mut contents, doc_name).await {
            Ok(response) => {
                if response.status_code() == 200 {
                    match bucket.head_object(doc_name).await {
                        Ok((head, _)) => {
                            if let Some(metadata) = head.metadata {
                                if let Some(hash) = metadata.get("ipfs-hash") {
                                    return Ok(hash.clone());
                                }
                            }
                            Err(DataAvailabilityError::IPFSError(
                                "Missing IPFS hash in metadata".into(),
                            ))
                        }
                        Err(e) => Err(DataAvailabilityError::IPFSError(e.to_string())),
                    }
                } else {
                    Err(DataAvailabilityError::IPFSError(format!(
                        "Upload failed with status: {}",
                        response.status_code()
                    )))
                }
            }
            Err(e) => Err(DataAvailabilityError::IPFSError(e.to_string())),
        }
    }
}

// Factory function to create a worker with real implementations
pub async fn create_data_availability_worker(
    config: DataAvailabilityConfig,
    pool: ConnectionPool<Core>,
    metrics: Arc<DataAvailabilityMetrics>,
) -> Result<DataAvailabilityWorker<impl IPFSService, impl MintlayerService>, DataAvailabilityError> {
    use super::services::{FourEverLandIPFSService, MintlayerRpcService};
    
    let ipfs_service = Arc::new(FourEverLandIPFSService::new(config.ipfs));
    let mintlayer_service = Arc::new(MintlayerRpcService::new(config.mintlayer));
    
    Ok(DataAvailabilityWorker::new(
        config.worker,
        pool,
        metrics,
        ipfs_service,
        mintlayer_service,
    ))
}
