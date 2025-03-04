use async_trait::async_trait;
use std::io::Cursor;

use super::{config::{IPFSConfig, MintlayerConfig}, error::DataAvailabilityError};

#[async_trait]
pub trait IPFSService: Send + Sync {
    async fn upload(&self, data: &[u8]) -> Result<String, DataAvailabilityError>;
}

#[async_trait]
pub trait MintlayerService: Send + Sync {
    async fn initialize_wallet(&self) -> Result<(), DataAvailabilityError>;
    async fn submit_hashes(&self, ipfs_hashes: &[String]) -> Result<String, DataAvailabilityError>;
}

pub struct FourEverLandIPFSService {
    config: IPFSConfig,
    client: reqwest::Client,
}

impl FourEverLandIPFSService {
    pub fn new(config: IPFSConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    async fn upload_to_ipfs(
        &self,
        bucket: &s3::Bucket,
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

#[async_trait]
impl IPFSService for FourEverLandIPFSService {
    async fn upload(&self, data: &[u8]) -> Result<String, DataAvailabilityError> {
        let credentials = s3::creds::Credentials::new(
            Some(&self.config.api_key),
            Some(&self.config.secret_key),
            None,
            None,
            None,
        )
        .map_err(|e| DataAvailabilityError::ConfigError(e.to_string()))?;
        
        let bucket = s3::Bucket::new(
            &self.config.bucket_name,
            s3::Region::Custom {
                region: self.config.region.clone(),
                endpoint: self.config.endpoint.clone(),
            },
            credentials,
        )
        .map_err(|e| DataAvailabilityError::ConfigError(e.to_string()))?;

        let doc_name = format!("op_{}", uuid::Uuid::new_v4());
        let contents = Cursor::new(data.to_vec());

        self.upload_to_ipfs(&bucket, &doc_name, contents).await
    }
}

pub struct MintlayerRpcService {
    config: MintlayerConfig,
    client: reqwest::Client,
}

impl MintlayerRpcService {
    pub fn new(config: MintlayerConfig) -> Self {
        Self {
            config,
            client: reqwest::Client::new(),
        }
    }

    fn get_auth_headers(&self) -> reqwest::header::HeaderMap {
        let mut headers = reqwest::header::HeaderMap::new();
        headers.insert("Content-Type", "application/json".parse().unwrap());
        
        if let (Some(username), Some(password)) = (&self.config.rpc_username, &self.config.rpc_password) {
            let creds = base64::engine::general_purpose::STANDARD
                .encode(format!("{}:{}", username, password));
            headers.insert(
                "Authorization",
                format!("Basic {}", creds).parse().unwrap(),
            );
        }
        
        headers
    }
}

#[async_trait]
impl MintlayerService for MintlayerRpcService {
    async fn initialize_wallet(&self) -> Result<(), DataAvailabilityError> {
        let headers = self.get_auth_headers();
        
        // Create wallet
        let payload = match &self.config.mnemonic {
            Some(mnemonic) => serde_json::json!({
                "method": "wallet_create",
                "params": {
                    "path": self.config.wallet_path,
                    "store_seed_phrase": true,
                    "mnemonic": mnemonic
                },
                "jsonrpc": "2.0",
                "id": 1,
            }),
            None => serde_json::json!({
                "method": "wallet_create",
                "params": {
                    "path": self.config.wallet_path,
                    "store_seed_phrase": true
                },
                "jsonrpc": "2.0",
                "id": 1,
            }),
        };

        let response = self.client
            .post(&self.config.rpc_url)
            .headers(headers.clone())
            .json(&payload)
            .send()
            .await;
        
        // Ignore errors as wallet might already exist
        
        // Open wallet
        let payload = serde_json::json!({
            "method": "wallet_open",
            "params": {
                "path": self.config.wallet_path,
            },
            "jsonrpc": "2.0",
            "id": 1,
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .headers(headers.clone())
            .json(&payload)
            .send()
            .await
            .map_err(|e| DataAvailabilityError::MintlayerError(format!("Failed to open wallet: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(DataAvailabilityError::MintlayerError(
                format!("Failed to open wallet: {}", response.status())
            ));
        }

        // Create address
        let payload = serde_json::json!({
            "method": "address_new",
            "params": {
                "account": 0,
            },
            "jsonrpc": "2.0",
            "id": 1,
        });

        let response = self.client
            .post(&self.config.rpc_url)
            .headers(headers)
            .json(&payload)
            .send()
            .await
            .map_err(|e| DataAvailabilityError::MintlayerError(format!("Failed to create address: {}", e)))?;
            
        if !response.status().is_success() {
            return Err(DataAvailabilityError::MintlayerError(
                format!("Failed to create address: {}", response.status())
            ));
        }

        Ok(())
    }

    async fn submit_hashes(&self, ipfs_hashes: &[String]) -> Result<String, DataAvailabilityError> {
        let headers = self.get_auth_headers();
        
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

        let response = self.client
            .post(&self.config.rpc_url)
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
} 