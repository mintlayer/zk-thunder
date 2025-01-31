use super::{
    circuit_breaker::CircuitBreaker, error::DataAvailabilityError, metrics::DataAvailabilityMetrics,
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

#[derive(Debug, Clone)]
pub struct WorkerConfig {
    pub ipfs_retry_base_delay: Duration,
    pub ipfs_retry_max_delay: Duration,
    pub ipfs_max_attempts: u32,
    pub mintlayer_retry_base: Duration,
    pub mintlayer_retry_max_delay: Duration,
    pub mintlayer_max_attempts: u32,
    pub cleanup_interval: Duration,
    pub cleanup_days_threshold: i32,
    pub batch_size: usize,
}

#[derive(Debug)]
pub struct DataAvailabilityWorker {
    config: WorkerConfig,
    pool: ConnectionPool<Core>,
    metrics: Arc<DataAvailabilityMetrics>,
    ipfs_circuit_breaker: Mutex<CircuitBreaker>,
    mintlayer_circuit_breaker: Mutex<CircuitBreaker>,
}

impl DataAvailabilityWorker {
    pub fn new(
        config: WorkerConfig,
        pool: ConnectionPool<Core>,
        metrics: Arc<DataAvailabilityMetrics>,
    ) -> Self {
        Self {
            ipfs_circuit_breaker: Mutex::new(CircuitBreaker::new(5, Duration::from_secs(300))),
            mintlayer_circuit_breaker: Mutex::new(CircuitBreaker::new(5, Duration::from_secs(300))),
            config,
            pool,
            metrics,
        }
    }

    async fn process_ipfs_operation(
        &self,
        op: &mut PendingIpfsOperation,
    ) -> Result<(), DataAvailabilityError> {
        if self.ipfs_circuit_breaker.lock().await.is_open() {
            self.metrics.circuit_breaker_trips.inc();
            return Err(DataAvailabilityError::CircuitBreakerOpenError(
                "IPFS".into(),
            ));
        }
        let start = Instant::now();
        let result = self.upload_to_ipfs_with_backoff(op).await;
        let duration = start.elapsed();
        self.metrics.ipfs_operation_duration.observe(duration);

        match result {
            Ok(hash) => {
                self.metrics.ipfs_success.inc();

                let mut conn = self
                    .pool
                    .connection_tagged("data_availability_worker")
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                let mut tx = conn
                    .start_transaction()
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                op.status = OperationStatus::Completed;
                op.ipfs_hash = Some(hash.clone());
                tx.data_availability_dal()
                    .update_ipfs_operations(op)
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                if op.requires_mintlayer {
                    self.queue_mintlayer_batch(tx, hash).await?;
                } else {
                    tx.commit()
                        .await
                        .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
                }

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
        let mut delay = self.config.ipfs_retry_base_delay;

        while op.attempts < self.config.ipfs_max_attempts {
            match self.setup_and_upload_to_ipfs(&op.data).await {
                Ok(hash) => return Ok(hash),
                Err(e) => {
                    op.attempts += 1;
                    self.metrics.ipfs_retry_count.inc();

                    if op.attempts >= self.config.ipfs_max_attempts {
                        return Err(e);
                    }

                    tracing::warn!("IPFS upload failed (attempt {}): {}", op.attempts, e);
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, self.config.ipfs_retry_max_delay);
                }
            }
        }

        Err(DataAvailabilityError::MaxRetriesExceededError(
            "IPFS".into(),
        ))
    }

    async fn setup_and_upload_to_ipfs(&self, data: &[u8]) -> Result<String, DataAvailabilityError> {
        let api_key = std::env::var("4EVERLAND_API_KEY")
            .map_err(|_| DataAvailabilityError::ConfigError("4EVERLAND_API_KEY not set".into()))?;
        let secret_key = std::env::var("4EVERLAND_SECRET_KEY").map_err(|_| {
            DataAvailabilityError::ConfigError("4EVERLAND_SECRET_KEY not set".into())
        })?;
        let bucket_name = std::env::var("4EVERLAND_BUCKET_NAME").map_err(|_| {
            DataAvailabilityError::ConfigError("4EVERLAND_BUCKET_NAME not set".into())
        })?;

        let credentials =
            s3::creds::Credentials::new(Some(&api_key), Some(&secret_key), None, None, None)
                .map_err(|e| DataAvailabilityError::ConfigError(e.to_string()))?;
        let bucket = Bucket::new(
            &bucket_name,
            s3::Region::Custom {
                region: "us-east-1".into(),
                endpoint: "https://endpoint.4everland.co".into(),
            },
            credentials,
        )
        .map_err(|e| DataAvailabilityError::ConfigError(e.to_string()))?;

        let doc_name = format!("op_{}", Uuid::new_v4());
        let contents = Cursor::new(data.to_vec());

        self.upload_to_ipfs(&bucket, &doc_name, contents).await
    }

    pub async fn run(self) {
        let mintlayer_rpc_url = std::env::var("ML_RPC_URL").expect("ML_RPC_URL not set");

        let mintlayer_rpc_username = std::env::var("ML_RPC_USERNAME");
        let mintlayer_rpc_password = std::env::var("ML_RPC_PASSWORD");
        let mnemonic = std::env::var("ML_MNEMONIC");

        let creds = match (mintlayer_rpc_username, mintlayer_rpc_password) {
            (Ok(username), Ok(password)) => {
                let creds = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                Some(format!("Basic {creds}"))
            }
            _ => None,
        };

        let client = reqwest::Client::new();
        let headers = {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Content-Type", "application/json".parse().unwrap());
            if let Some(credentials) = creds {
                headers.insert("Authorization", credentials.as_str().parse().unwrap());
            }
            headers
        };

        let payload = match mnemonic {
            Ok(mnemonic) => serde_json::json!({
                "method": "wallet_create",
                "params": {
                    "path": "/home/mintlayer/wallet.dat",
                    "store_seed_phrase": true,
                    "mnemonic": mnemonic
                },
                "jsonrpc": "2.0",
                "id": 1,
            }),
            Err(_) => serde_json::json!({
                "method": "wallet_create",
                "params": {
                    "path": "/home/mintlayer/wallet.dat",
                    "store_seed_phrase": true
                },
                "jsonrpc": "2.0",
                "id": 1,
            }),
        };

        let _ = client
            .post(&mintlayer_rpc_url)
            .headers(headers.clone())
            .json(&payload)
            .send()
            .await;

        let payload = serde_json::json!({
            "method": "wallet_open",
            "params": {
                "path": "/home/mintlayer/wallet.dat",
            },
            "jsonrpc": "2.0",
            "id": 1,
        });

        let _ = client
            .post(&mintlayer_rpc_url)
            .headers(headers.clone())
            .json(&payload)
            .send()
            .await;

        let payload = serde_json::json!({
            "method": "address_new",
            "params": {
                "account": 0,
            },
            "jsonrpc": "2.0",
            "id": 1,
        });

        let _ = client
            .post(&mintlayer_rpc_url)
            .headers(headers)
            .json(&payload)
            .send()
            .await;

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

    async fn process_mintlayer_batch(
        &self,
        batch: &mut PendingMintlayerBatch,
    ) -> Result<(), DataAvailabilityError> {
        if self.mintlayer_circuit_breaker.lock().await.is_open() {
            self.metrics.circuit_breaker_trips.inc();
            return Err(DataAvailabilityError::CircuitBreakerOpenError(
                "Mintlayer".into(),
            ));
        }

        let start = Instant::now();
        let result = self.submit_to_mintlayer_with_backoff(batch).await;
        let duration = start.elapsed();
        self.metrics.mintlayer_operation_duration.observe(duration);

        match result {
            Ok(tx_hash) => {
                self.metrics.mintlayer_success.inc();

                let mut conn = self
                    .pool
                    .connection_tagged("data_availability_worker")
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
                let mut tx = conn
                    .start_transaction()
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

                batch.status = OperationStatus::Completed;
                batch.tx_hash = Some(tx_hash);

                tx.data_availability_dal()
                    .update_mintlayer_batch(batch)
                    .await
                    .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
                tx.commit()
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

    async fn submit_to_mintlayer_with_backoff(
        &self,
        batch: &mut PendingMintlayerBatch,
    ) -> Result<String, DataAvailabilityError> {
        let mut delay = self.config.mintlayer_retry_base;

        while batch.attempts < self.config.mintlayer_max_attempts {
            match self.submit_to_mintlayer(&batch.ipfs_hashes).await {
                Ok(tx_hash) => return Ok(tx_hash),
                Err(e) => {
                    batch.attempts += 1;
                    self.metrics.mintlayer_retry_count.inc();

                    if batch.attempts >= self.config.mintlayer_max_attempts {
                        return Err(e);
                    }

                    tracing::warn!(
                        "Mintlayer submission failed (attempt {}): {}",
                        batch.attempts,
                        e
                    );
                    tokio::time::sleep(delay).await;
                    delay = std::cmp::min(delay * 2, self.config.mintlayer_retry_max_delay);
                }
            }
        }

        Err(DataAvailabilityError::MaxRetriesExceededError(
            "Mintlayer".into(),
        ))
    }

    async fn submit_to_mintlayer(
        &self,
        ipfs_hashes: &[String],
    ) -> Result<String, DataAvailabilityError> {
        let mintlayer_rpc_url = std::env::var("ML_RPC_URL")
            .map_err(|_| DataAvailabilityError::ConfigError("ML_RPC_URL not set".into()))?;

        let mintlayer_rpc_username = std::env::var("ML_RPC_USERNAME");
        let mintlayer_rpc_password = std::env::var("ML_RPC_PASSWORD");

        let creds = match (mintlayer_rpc_username, mintlayer_rpc_password) {
            (Ok(username), Ok(password)) => {
                let creds = base64::engine::general_purpose::STANDARD
                    .encode(format!("{username}:{password}"));
                Some(format!("Basic {creds}"))
            }
            _ => None,
        };

        let client = reqwest::Client::new();
        let headers = {
            let mut headers = reqwest::header::HeaderMap::new();
            headers.insert("Content-Type", "application/json".parse().unwrap());
            if let Some(credentials) = creds {
                headers.insert("Authorization", credentials.as_str().parse().unwrap());
            }
            headers
        };
        let payload = serde_json::json!({
            "method": "address_deposit_data",
            "params": {
                "data": hex::encode(ipfs_hashes.join(",")),
                "account": 0,
                "options": {},
            },
            "jsonrpc": "2.0",
            "id": 1,
        });

        let response = client
            .post(&mintlayer_rpc_url)
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .map_err(|e| DataAvailabilityError::MintlayerError(e.to_string()))?;

        if !response.status().is_success() {
            return Err(DataAvailabilityError::MintlayerError(format!(
                "Request failed with status: {}",
                response.status()
            )));
        }

        let response_text = response
            .text()
            .await
            .map_err(|e| DataAvailabilityError::MintlayerError(e.to_string()))?;
        let response_json: serde_json::Value = serde_json::from_str(&response_text)
            .map_err(|e| DataAvailabilityError::MintlayerError(e.to_string()))?;

        tracing::info!(
            "add root digest to mintlayer with L1 tx_info: {}",
            serde_json::to_string(&response_json).unwrap()
        );
        match response_json.get("result") {
            Some(tx_hash) => Ok(tx_hash.as_str().unwrap_or("").to_string()),
            None => Err(DataAvailabilityError::MintlayerError(
                "No tx_hash in response".into(),
            )),
        }
    }

    async fn queue_mintlayer_batch(
        &self,
        mut tx: Connection<'_, Core>,
        hash: String,
    ) -> Result<(), DataAvailabilityError> {
        let mut conn = self
            .pool
            .connection_tagged("data_availability_worker")
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;
        let mut batches = conn
            .data_availability_dal()
            .get_pending_mintlayer_batches()
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        let pending_mintlayer_batch = &mut PendingMintlayerBatch::new();

        let batch = batches
            .iter_mut()
            .find(|b| {
                b.status == OperationStatus::Pending && b.ipfs_hashes.len() < self.config.batch_size
            })
            .unwrap_or_else(|| pending_mintlayer_batch);
        batch.ipfs_hashes.push(hash);

        if batch.ipfs_hashes.len() >= self.config.batch_size {
            batch.status = OperationStatus::Pending;
        }

        tx.data_availability_dal()
            .update_mintlayer_batch(batch)
            .await
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))?;

        tx.commit()
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
