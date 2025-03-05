use super::{
    circuit_breaker::{CircuitBreaker, CircuitBreakerState},
    config::{DataAvailabilityConfig, WorkerConfig},
    error::DataAvailabilityError,
    metrics::DataAvailabilityMetrics,
    retry::with_exponential_backoff,
    services::{IPFSService, MintlayerService},
};
use std::{sync::Arc, time::Instant};
use chrono::Utc;
use tokio::sync::Mutex;
use tokio::time::Duration;
use uuid::Uuid;
use zksync_dal::{
    data_availability_dal::{OperationStatus, PendingIpfsOperation, PendingMintlayerBatch},
    Connection, ConnectionPool, Core, CoreDal,
};
use serde_json;

#[derive(Debug)]
pub struct DataAvailabilityWorker<I: IPFSService + 'static, M: MintlayerService + 'static> {
    config: WorkerConfig,
    pool: ConnectionPool<Core>,
    metrics: Arc<DataAvailabilityMetrics>,
    ipfs_service: Arc<I>,
    mintlayer_service: Arc<M>,
    ipfs_circuit_breaker: Mutex<CircuitBreaker>,
    mintlayer_circuit_breaker: Mutex<CircuitBreaker>,
}

impl<I: IPFSService + 'static, M: MintlayerService + 'static> DataAvailabilityWorker<I, M> {
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
                op.status = OperationStatus::Completed;
                op.ipfs_hash = Some(hash.clone());

                let mut conn = self
                    .pool
                    .connection_tagged("data_availability_worker")
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                conn.data_availability_dal()
                    .update_ipfs_operations(op)
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                self.queue_mintlayer_batch(conn, hash).await?;
                
                Ok(())
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
                self.ipfs_service.upload(&data).await
            },
            self.config.ipfs_retry_base_delay,
            self.config.ipfs_retry_max_delay,
            self.config.ipfs_max_attempts,
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

        if batch.group_ipfs_hash.is_none() {
            let data = serde_json::to_string(&batch.ipfs_hashes)
                .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

            let root_hash = self.ipfs_service.upload(data.as_bytes()).await?;
            batch.group_ipfs_hash = Some(root_hash);

            let mut conn = self.pool.connection_tagged("data_availability_worker").await
                .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
            conn.data_availability_dal().update_mintlayer_batch(batch).await
                .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
        }

        let start = Instant::now();
        let result = self.submit_root_to_mintlayer_with_backoff(batch).await;
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

                batch.status = OperationStatus::Completed;
                batch.tx_hash = Some(tx_hash);

                conn.data_availability_dal()
                    .update_mintlayer_batch(batch)
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                Ok(())
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

    async fn submit_root_to_mintlayer_with_backoff(
        &self,
        batch: &mut PendingMintlayerBatch,
    ) -> Result<String, DataAvailabilityError> {
        let root_hash = batch.group_ipfs_hash.clone()
            .ok_or_else(|| DataAvailabilityError::DatabaseError("Root IPFS hash is missing".into()))?;

        let hashes = vec![root_hash];
        
        with_exponential_backoff(
            || async {
                self.mintlayer_service.submit_hashes(&hashes).await
            },
            self.config.mintlayer_retry_base,
            self.config.mintlayer_retry_max_delay,
            self.config.mintlayer_max_attempts,
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
                            if batch.ipfs_hashes.len() >= self.config.batch_size {
                                if let Err(e) = self.process_mintlayer_batch(&mut batch).await {
                                    tracing::error!(
                                        "Failed to process Mintlayer batch {}: {}",
                                        batch.id,
                                        e
                                    );
                                }
                            }
                        }
                    },
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
        let mut batches = tx
            .data_availability_dal()
            .get_pending_mintlayer_batches()
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        let batch_index = batches.iter().position(|b| {
            b.status == OperationStatus::Pending && b.ipfs_hashes.len() < self.config.batch_size
        });

        let batch = if let Some(index) = batch_index {
            &mut batches[index]
        } else {
            let new_batch = PendingMintlayerBatch {
                id: Uuid::new_v4(),
                ipfs_hashes: Vec::new(),
                attempts: 0,
                last_attempt: None,
                created_at: Utc::now(),
                status: OperationStatus::Pending,
                tx_hash: None,
                group_ipfs_hash: None,
            };
            batches.push(new_batch);
            batches.last_mut().unwrap()
        };

        batch.ipfs_hashes.push(hash);

        tx.data_availability_dal()
            .update_mintlayer_batch(batch)
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        tx.commit()
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        Ok(())
    }
}

pub async fn create_data_availability_worker(
    config: DataAvailabilityConfig,
    pool: ConnectionPool<Core>,
    metrics: Arc<DataAvailabilityMetrics>,
) -> Result<DataAvailabilityWorker<Box<dyn IPFSService>, Box<dyn MintlayerService>>, DataAvailabilityError> {
    use super::services::{FourEverLandIPFSService, MintlayerRpcService};
    
    let ipfs_service: Box<dyn IPFSService> = Box::new(FourEverLandIPFSService::new(config.ipfs));
    let mintlayer_service: Box<dyn MintlayerService> = Box::new(MintlayerRpcService::new(config.mintlayer));
    
    Ok(DataAvailabilityWorker::new(
        config.worker,
        pool,
        metrics,
        Arc::new(ipfs_service),
        Arc::new(mintlayer_service),
    ))
}