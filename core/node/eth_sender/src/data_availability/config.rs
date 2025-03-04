use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IPFSConfig {
    pub api_key: String,
    pub secret_key: String,
    pub bucket_name: String,
    pub region: String,
    pub endpoint: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MintlayerConfig {
    pub rpc_url: String,
    pub rpc_username: Option<String>,
    pub rpc_password: Option<String>,
    pub mnemonic: Option<String>,
    pub wallet_path: String,
}

#[derive(Debug, Clone)]
pub struct DataAvailabilityConfig {
    pub worker: WorkerConfig,
    pub ipfs: IPFSConfig,
    pub mintlayer: MintlayerConfig,
}

impl DataAvailabilityConfig {
    pub fn from_env() -> Result<Self, String> {
        let worker = WorkerConfig {
            ipfs_retry_base_delay: Duration::from_secs(
                std::env::var("IPFS_RETRY_BASE_DELAY_SECS")
                    .unwrap_or_else(|_| "5".to_string())
                    .parse()
                    .map_err(|e| format!("Invalid IPFS_RETRY_BASE_DELAY_SECS: {}", e))?,
            ),
            ipfs_retry_max_delay: Duration::from_secs(
                std::env::var("IPFS_RETRY_MAX_DELAY_SECS")
                    .unwrap_or_else(|_| "300".to_string())
                    .parse()
                    .map_err(|e| format!("Invalid IPFS_RETRY_MAX_DELAY_SECS: {}", e))?,
            ),
            ipfs_max_attempts: std::env::var("IPFS_MAX_ATTEMPTS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|e| format!("Invalid IPFS_MAX_ATTEMPTS: {}", e))?,
            mintlayer_retry_base: Duration::from_secs(
                std::env::var("MINTLAYER_RETRY_BASE_SECS")
                    .unwrap_or_else(|_| "5".to_string())
                    .parse()
                    .map_err(|e| format!("Invalid MINTLAYER_RETRY_BASE_SECS: {}", e))?,
            ),
            mintlayer_retry_max_delay: Duration::from_secs(
                std::env::var("MINTLAYER_RETRY_MAX_DELAY_SECS")
                    .unwrap_or_else(|_| "300".to_string())
                    .parse()
                    .map_err(|e| format!("Invalid MINTLAYER_RETRY_MAX_DELAY_SECS: {}", e))?,
            ),
            mintlayer_max_attempts: std::env::var("MINTLAYER_MAX_ATTEMPTS")
                .unwrap_or_else(|_| "5".to_string())
                .parse()
                .map_err(|e| format!("Invalid MINTLAYER_MAX_ATTEMPTS: {}", e))?,
            cleanup_interval: Duration::from_secs(
                std::env::var("CLEANUP_INTERVAL_SECS")
                    .unwrap_or_else(|_| "3600".to_string())
                    .parse()
                    .map_err(|e| format!("Invalid CLEANUP_INTERVAL_SECS: {}", e))?,
            ),
            cleanup_days_threshold: std::env::var("CLEANUP_DAYS_THRESHOLD")
                .unwrap_or_else(|_| "30".to_string())
                .parse()
                .map_err(|e| format!("Invalid CLEANUP_DAYS_THRESHOLD: {}", e))?,
            batch_size: std::env::var("BATCH_SIZE")
                .unwrap_or_else(|_| "10".to_string())
                .parse()
                .map_err(|e| format!("Invalid BATCH_SIZE: {}", e))?,
        };

        let ipfs = IPFSConfig {
            api_key: std::env::var("4EVERLAND_API_KEY")
                .map_err(|_| "4EVERLAND_API_KEY not set".to_string())?,
            secret_key: std::env::var("4EVERLAND_SECRET_KEY")
                .map_err(|_| "4EVERLAND_SECRET_KEY not set".to_string())?,
            bucket_name: std::env::var("4EVERLAND_BUCKET_NAME")
                .map_err(|_| "4EVERLAND_BUCKET_NAME not set".to_string())?,
            region: std::env::var("4EVERLAND_REGION").unwrap_or_else(|_| "us-east-1".to_string()),
            endpoint: std::env::var("4EVERLAND_ENDPOINT")
                .unwrap_or_else(|_| "https://endpoint.4everland.co".to_string()),
        };

        let mintlayer = MintlayerConfig {
            rpc_url: std::env::var("ML_RPC_URL").map_err(|_| "ML_RPC_URL not set".to_string())?,
            rpc_username: std::env::var("ML_RPC_USERNAME").ok(),
            rpc_password: std::env::var("ML_RPC_PASSWORD").ok(),
            mnemonic: std::env::var("ML_MNEMONIC").ok(),
            wallet_path: std::env::var("ML_WALLET_PATH")
                .unwrap_or_else(|_| "/home/mintlayer/wallet.dat".to_string()),
        };

        Ok(DataAvailabilityConfig {
            worker,
            ipfs,
            mintlayer,
        })
    }
} 