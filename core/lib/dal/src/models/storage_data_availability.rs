use chrono::NaiveDateTime;
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::fmt::Display;
use uuid::Uuid;
use zksync_types::{pubdata_da::DataAvailabilityBlob, L1BatchNumber};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum OperationStatus {
    Pending,
    InProgress,
    Completed,
    Failed(String),
}

impl Display for OperationStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let status = match self {
            OperationStatus::Pending => "pending",
            OperationStatus::InProgress => "in_progress",
            OperationStatus::Completed => "completed",
            OperationStatus::Failed(_) => "failed",
        };

        write!(f, "{}", status)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub enum OperationType {
    Commit,
    Proof,
    Execute,
}

impl OperationType {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s {
            "commit" => Ok(Self::Commit),
            "proof" => Ok(Self::Proof),
            "execute" => Ok(Self::Execute),
            default => Err(format!("Unrecognized operation type: {}", default)),
        }
    }
}

impl std::fmt::Display for OperationType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Commit => write!(f, "commit"),
            Self::Proof => write!(f, "proof"),
            Self::Execute => write!(f, "execute"),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingIpfsOperation {
    pub id: Uuid,
    pub operation_type: OperationType,
    pub data: Vec<u8>,
    pub attempts: u32,
    pub last_attempt: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub status: OperationStatus,
    pub ipfs_hash: Option<String>,
    pub requires_mintlayer: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingMintlayerBatch {
    pub id: Uuid,
    pub ipfs_hashes: Vec<String>,
    pub attempts: u32,
    pub last_attempt: Option<DateTime<Utc>>,
    pub created_at: DateTime<Utc>,
    pub status: OperationStatus,
    pub tx_hash: Option<String>,
}

impl PendingMintlayerBatch {
    pub fn new() -> Self {
        Self {
            id: Uuid::new_v4(),
            ipfs_hashes: Vec::new(),
            attempts: 0,
            last_attempt: None,
            created_at: Utc::now(),
            status: OperationStatus::Pending,
            tx_hash: None,
        }
    }
}

/// Represents a blob in the data availability layer.
#[derive(Debug, Clone)]
pub(crate) struct StorageDABlob {
    pub l1_batch_number: i64,
    pub blob_id: String,
    pub inclusion_data: Option<Vec<u8>>,
    pub sent_at: NaiveDateTime,
}

impl From<StorageDABlob> for DataAvailabilityBlob {
    fn from(blob: StorageDABlob) -> DataAvailabilityBlob {
        DataAvailabilityBlob {
            l1_batch_number: L1BatchNumber(blob.l1_batch_number as u32),
            blob_id: blob.blob_id,
            inclusion_data: blob.inclusion_data,
            sent_at: blob.sent_at.and_utc(),
        }
    }
}

/// A small struct used to store a batch and its data availability, which are retrieved from the database.
#[derive(Debug)]
pub struct L1BatchDA {
    pub pubdata: Vec<u8>,
    pub l1_batch_number: L1BatchNumber,
}
