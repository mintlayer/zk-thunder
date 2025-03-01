use zksync_db_connection::{
    connection::Connection,
    error::DalResult,
    instrument::{InstrumentExt, Instrumented},
};
use zksync_types::{pubdata_da::DataAvailabilityBlob, L1BatchNumber};

use crate::{
    models::storage_data_availability::{L1BatchDA, StorageDABlob},
    Core,
};

pub use crate::models::storage_data_availability::{
    OperationStatus, OperationType, PendingIpfsOperation, PendingMintlayerBatch,
};

const MAX_RETRY_ATTEMPTS: i32 = 10;
const MAX_BATCH_SIZE: i64 = 100;

#[derive(Debug)]
pub struct DataAvailabilityDal<'a, 'c> {
    pub(crate) storage: &'a mut Connection<'c, Core>,
}

impl DataAvailabilityDal<'_, '_> {
    /// Inserts the blob_id for the given L1 batch. If the blob_id is already present,
    /// verifies that it matches the one provided in the function arguments
    /// (preventing the same L1 batch from being stored twice)
    pub async fn insert_l1_batch_da(
        &mut self,
        number: L1BatchNumber,
        blob_id: &str,
        sent_at: chrono::NaiveDateTime,
    ) -> DalResult<()> {
        let update_result = sqlx::query!(
            r#"
            INSERT INTO
                data_availability (l1_batch_number, blob_id, sent_at, created_at, updated_at)
            VALUES
                ($1, $2, $3, NOW(), NOW())
            ON CONFLICT DO NOTHING
            "#,
            i64::from(number.0),
            blob_id,
            sent_at,
        )
        .instrument("insert_l1_batch_da")
        .with_arg("number", &number)
        .with_arg("blob_id", &blob_id)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!(
                "L1 batch #{number}: DA blob_id wasn't updated as it's already present"
            );

            let instrumentation =
                Instrumented::new("get_matching_batch_da_blob_id").with_arg("number", &number);

            // Batch was already processed. Verify that existing DA blob_id matches
            let query = sqlx::query!(
                r#"
                SELECT
                    blob_id
                FROM
                    data_availability
                WHERE
                    l1_batch_number = $1
                "#,
                i64::from(number.0),
            );

            let matched: String = instrumentation
                .clone()
                .with(query)
                .report_latency()
                .fetch_one(self.storage)
                .await?
                .blob_id;

            if matched != *blob_id.to_string() {
                let err = instrumentation.constraint_error(anyhow::anyhow!(
                    "Error storing DA blob id. DA blob_id {blob_id} for L1 batch #{number} does not match the expected value"
                ));
                return Err(err);
            }
        }
        Ok(())
    }

    /// Saves the inclusion data for the given L1 batch. If the inclusion data is already present,
    /// verifies that it matches the one provided in the function arguments
    /// (meaning that the inclusion data corresponds to the same DA blob)
    pub async fn save_l1_batch_inclusion_data(
        &mut self,
        number: L1BatchNumber,
        da_inclusion_data: &[u8],
    ) -> DalResult<()> {
        let update_result = sqlx::query!(
            r#"
            UPDATE data_availability
            SET
                inclusion_data = $1,
                updated_at = NOW()
            WHERE
                l1_batch_number = $2
                AND inclusion_data IS NULL
            "#,
            da_inclusion_data,
            i64::from(number.0),
        )
        .instrument("save_l1_batch_da_data")
        .with_arg("number", &number)
        .report_latency()
        .execute(self.storage)
        .await?;

        if update_result.rows_affected() == 0 {
            tracing::debug!("L1 batch #{number}: DA data wasn't updated as it's already present");

            let instrumentation =
                Instrumented::new("get_matching_batch_da_data").with_arg("number", &number);

            // Batch was already processed. Verify that existing DA data matches
            let query = sqlx::query!(
                r#"
                SELECT
                    inclusion_data
                FROM
                    data_availability
                WHERE
                    l1_batch_number = $1
                "#,
                i64::from(number.0),
            );

            let matched: Option<Vec<u8>> = instrumentation
                .clone()
                .with(query)
                .report_latency()
                .fetch_one(self.storage)
                .await?
                .inclusion_data;

            if matched.unwrap_or_default() != da_inclusion_data.to_vec() {
                let err = instrumentation.constraint_error(anyhow::anyhow!(
                    "Error storing DA inclusion data. DA data for L1 batch #{number} does not match the one provided before"
                ));
                return Err(err);
            }
        }
        Ok(())
    }

    /// Assumes that the L1 batches are sorted by number, and returns the first one that is ready for DA dispatch.
    pub async fn get_first_da_blob_awaiting_inclusion(
        &mut self,
    ) -> DalResult<Option<DataAvailabilityBlob>> {
        Ok(sqlx::query_as!(
            StorageDABlob,
            r#"
            SELECT
                l1_batch_number,
                blob_id,
                inclusion_data,
                sent_at
            FROM
                data_availability
            WHERE
                inclusion_data IS NULL
            ORDER BY
                l1_batch_number
            LIMIT
                1
            "#,
        )
        .instrument("get_first_da_blob_awaiting_inclusion")
        .fetch_optional(self.storage)
        .await?
        .map(DataAvailabilityBlob::from))
    }

    /// Fetches the pubdata and `l1_batch_number` for the L1 batches that are ready for DA dispatch.
    pub async fn get_ready_for_da_dispatch_l1_batches(
        &mut self,
        limit: usize,
    ) -> DalResult<Vec<L1BatchDA>> {
        let rows = sqlx::query!(
            r#"
            SELECT
                number,
                pubdata_input
            FROM
                l1_batches
                LEFT JOIN data_availability ON data_availability.l1_batch_number = l1_batches.number
            WHERE
                eth_commit_tx_id IS NULL
                AND number != 0
                AND data_availability.blob_id IS NULL
                AND pubdata_input IS NOT NULL
            ORDER BY
                number
            LIMIT
                $1
            "#,
            limit as i64,
        )
        .instrument("get_ready_for_da_dispatch_l1_batches")
        .with_arg("limit", &limit)
        .fetch_all(self.storage)
        .await?;

        Ok(rows
            .into_iter()
            .map(|row| L1BatchDA {
                // `unwrap` is safe here because we have a `WHERE` clause that filters out `NULL` values
                pubdata: row.pubdata_input.unwrap(),
                l1_batch_number: L1BatchNumber(row.number as u32),
            })
            .collect())
    }

    pub async fn get_pending_ipfs_operations(self) -> DalResult<Vec<PendingIpfsOperation>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                id, operation_type, data, attempts, 
                last_attempt as "last_attempt", created_at,
                status::text as "status!", ipfs_hash,
                requires_mintlayer
            FROM pending_ipfs_operations
            WHERE status::text = 'pending'
            OR (status::text = 'failed' AND attempts < $1)
            ORDER BY created_at ASC
            LIMIT $2
            "#,
            MAX_RETRY_ATTEMPTS,
            MAX_BATCH_SIZE
        )
        .instrument("get_pending_ipfs_operations")
        .with_arg("MAX_RETRY_ATTEMPTS", &MAX_RETRY_ATTEMPTS)
        .with_arg("MAX_BATCH_SIZE", &MAX_BATCH_SIZE)
        .fetch_all(self.storage)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PendingIpfsOperation {
                    id: row.id,
                    operation_type: OperationType::from_str(&row.operation_type)
                        .expect("Invalid operation_type"),
                    data: row.data,
                    attempts: row.attempts as u32,
                    last_attempt: row.last_attempt,
                    created_at: row.created_at,
                    status: match row.status.as_str() {
                        "pending" => OperationStatus::Pending,
                        "in_progress" => OperationStatus::InProgress,
                        "completed" => OperationStatus::Completed,
                        "failed" => OperationStatus::Failed("".to_string()),
                        _ => panic!("Invalid operation_type"),
                    },
                    ipfs_hash: row.ipfs_hash,
                    requires_mintlayer: row.requires_mintlayer,
                })
            })
            .collect()
    }

    pub async fn get_pending_mintlayer_batches(self) -> DalResult<Vec<PendingMintlayerBatch>> {
        let rows = sqlx::query!(
            r#"
            SELECT 
                id, ipfs_hashes, attempts, last_attempt,
                created_at, status::text as "status!", tx_hash
            FROM pending_mintlayer_batches
            WHERE status::text = 'pending'
            OR (status::text = 'failed' AND attempts < $1)
            ORDER BY created_at ASC
            LIMIT $2
            "#,
            MAX_RETRY_ATTEMPTS,
            MAX_BATCH_SIZE
        )
        .instrument("get_pending_mintlayer_batches")
        .with_arg("MAX_RETRY_ATTEMPTS", &MAX_RETRY_ATTEMPTS)
        .with_arg("MAX_BATCH_SIZE", &MAX_BATCH_SIZE)
        .fetch_all(self.storage)
        .await?;

        rows.into_iter()
            .map(|row| {
                Ok(PendingMintlayerBatch {
                    id: row.id,
                    ipfs_hashes: row.ipfs_hashes,
                    attempts: row.attempts as u32,
                    last_attempt: row.last_attempt,
                    created_at: row.created_at,
                    status: match row.status.as_str() {
                        "pending" => OperationStatus::Pending,
                        "in_progress" => OperationStatus::InProgress,
                        "completed" => OperationStatus::Completed,
                        "failed" => OperationStatus::Failed("".to_string()),
                        _ => panic!("Invalid operation_status"),
                    },
                    tx_hash: row.tx_hash,
                })
            })
            .collect()
    }

    pub async fn update_ipfs_operations<'a>(self, op: &PendingIpfsOperation) -> DalResult<()> {
        sqlx::query!(
            r#"
            UPDATE pending_ipfs_operations
            SET status = $1::text::operation_status, attempts = $2, last_attempt = $3, ipfs_hash = $4
            WHERE id = $5
            "#,
            op.status.to_string(),
            op.attempts as i32,
            op.last_attempt,
            op.ipfs_hash,
            op.id
        ).instrument("update_ipfs_operations")
        .with_arg("status", &op.status.to_string())
        .with_arg("attempts", &op.attempts)
        .with_arg("last_attempt", &op.last_attempt)
        .with_arg("ipfs_hash", &op.ipfs_hash)
        .with_arg("id", &op.id)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn update_mintlayer_batch<'a>(self, batch: &PendingMintlayerBatch) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO pending_mintlayer_batches (
                id, ipfs_hashes, status, attempts, last_attempt, created_at
            ) VALUES ($1, $2, $3::text::operation_status, $4, $5, $6)
            ON CONFLICT (id) DO UPDATE SET 
                ipfs_hashes = $2,
                status = $3::text::operation_status
            "#,
            batch.id,
            &batch.ipfs_hashes,
            batch.status.to_string(),
            batch.attempts as i32,
            batch.last_attempt,
            batch.created_at,
        )
        .instrument("update_mintlayer_batch")
        .with_arg("id", &batch.id)
        .with_arg("ipfs_hashes", &batch.ipfs_hashes)
        .with_arg("status", &batch.status)
        .with_arg("attempts", &batch.attempts)
        .with_arg("last_attempt", &batch.last_attempt)
        .with_arg("created_at", &batch.created_at)
        .execute(self.storage)
        .await?;
        Ok(())
    }

    pub async fn cleanup_old_operations(self, days_old: i32) -> DalResult<()> {
        let mut tx = self.storage.start_transaction().await?;

        sqlx::query!(
            r#"
        DELETE FROM pending_ipfs_operations
        WHERE created_at < NOW() - make_interval(days => $1)
        AND (status = 'completed' OR attempts >= $2)
        "#,
            days_old,
            MAX_RETRY_ATTEMPTS
        )
        .instrument("cleanup_old_operations")
        .with_arg("days old", &days_old)
        .with_arg("MAX_RETRY_ATTEMPTS", &MAX_RETRY_ATTEMPTS)
        .execute(&mut tx)
        .await?;

        sqlx::query!(
            r#"
        DELETE FROM pending_mintlayer_batches
        WHERE created_at < NOW() - make_interval(days => $1)
        AND (status = 'completed' OR attempts >= $2)
        "#,
            days_old,
            MAX_RETRY_ATTEMPTS
        )
        .instrument("cleanup_old_operations")
        .with_arg("days old", &days_old)
        .with_arg("MAX_RETRY_ATTEMPTS", &MAX_RETRY_ATTEMPTS)
        .execute(&mut tx)
        .await?;

        tx.commit().await?;
        Ok(())
    }

    pub async fn save_pending_operation(self, op: &PendingIpfsOperation) -> DalResult<()> {
        sqlx::query!(
            r#"
            INSERT INTO pending_ipfs_operations (
                id, operation_type, data, attempts,
                last_attempt, created_at, status, ipfs_hash,
                requires_mintlayer
            ) VALUES ($1, $2, $3, $4, $5, $6, $7::text::operation_status, $8, $9) 
            "#,
            op.id,
            op.operation_type.to_string(),
            op.data,
            op.attempts as i32,
            op.last_attempt,
            op.created_at,
            op.status.to_string(),
            op.ipfs_hash,
            op.requires_mintlayer
        )
        .instrument("save_pending_operation")
        .with_arg("id", &op.id)
        .with_arg("operation type", &op.operation_type)
        .with_arg("data", &op.data)
        .with_arg("attempts", &op.attempts)
        .with_arg("last attempt", &op.last_attempt)
        .with_arg("created at", &op.created_at)
        .with_arg("status", &op.status)
        .with_arg("ipfs hash", &op.ipfs_hash)
        .with_arg("requires mintlayer", &op.requires_mintlayer)
        .execute(self.storage)
        .await?;
        Ok(())
    }
}
