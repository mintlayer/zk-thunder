use std::{sync::Arc, time::Duration};

use chrono::Utc;
use serde_json;
use tokio::sync::watch;
use zksync_config::configs::eth_sender::SenderConfig;
use zksync_contracts::BaseSystemContractsHashes;
use zksync_dal::{
    data_availability_dal::{OperationStatus, OperationType, PendingIpfsOperation},
    Connection, ConnectionPool, Core, CoreDal,
};
use zksync_eth_client::{BoundEthInterface, CallFunctionArgs, EthInterface};
use zksync_l1_contract_interface::{
    i_executor::{
        commit::kzg::{KzgInfo, ZK_SYNC_BYTES_PER_BLOB},
        methods::CommitBatches,
    },
    multicall3::{Multicall3Call, Multicall3Result},
    Tokenizable, Tokenize,
};
use zksync_shared_metrics::BlockL1Stage;
use zksync_types::{
    aggregated_operations::AggregatedActionType,
    commitment::{L1BatchWithMetadata, SerializeCommitment},
    eth_sender::{EthTx, EthTxBlobSidecar, EthTxBlobSidecarV1, SidecarBlobV1},
    ethabi::{Function, Token},
    l2_to_l1_log::UserL2ToL1Log,
    protocol_version::{L1VerifierConfig, VerifierParams, PACKED_SEMVER_MINOR_MASK},
    pubdata_da::PubdataDA,
    web3::{contract::Error as Web3ContractError, BlockNumber},
    Address, L2ChainId, ProtocolVersionId, H256, U256,
};

use super::aggregated_operations::AggregatedOperation;
use crate::{
    data_availability::{
        error::DataAvailabilityError,
        metrics::DataAvailabilityMetrics,
        worker::{DataAvailabilityWorker, WorkerConfig},
    },
    metrics::{PubdataKind, METRICS},
    utils::agg_l1_batch_base_cost,
    zksync_functions::ZkSyncFunctions,
    Aggregator, EthSenderError,
};

/// Data queried from L1 using multicall contract.
#[derive(Debug)]
pub struct MulticallData {
    pub base_system_contracts_hashes: BaseSystemContractsHashes,
    pub verifier_params: VerifierParams,
    pub verifier_address: Address,
    pub protocol_version_id: ProtocolVersionId,
}

/// The component is responsible for aggregating l1 batches into eth_txs:
/// Such as CommitBlocks, PublishProofBlocksOnchain and ExecuteBlock
/// These eth_txs will be used as a queue for generating signed txs and send them later
#[derive(Debug)]
pub struct EthTxAggregator {
    aggregator: Aggregator,
    eth_client: Box<dyn BoundEthInterface>,
    config: SenderConfig,
    timelock_contract_address: Address,
    l1_multicall3_address: Address,
    pub(super) state_transition_chain_contract: Address,
    functions: ZkSyncFunctions,
    base_nonce: u64,
    base_nonce_custom_commit_sender: Option<u64>,
    rollup_chain_id: L2ChainId,
    /// If set to `Some` node is operating in the 4844 mode with two operator
    /// addresses at play: the main one and the custom address for sending commit
    /// transactions. The `Some` then contains the address of this custom operator
    /// address.
    custom_commit_sender_addr: Option<Address>,
    pool: ConnectionPool<Core>,
    data_availability_worker: Option<DataAvailabilityWorker>,
    metrics: Arc<DataAvailabilityMetrics>,
}

struct TxData {
    calldata: Vec<u8>,
    sidecar: Option<EthTxBlobSidecar>,
}

impl EthTxAggregator {
    #[allow(clippy::too_many_arguments)]
    pub async fn new(
        pool: ConnectionPool<Core>,
        config: SenderConfig,
        aggregator: Aggregator,
        eth_client: Box<dyn BoundEthInterface>,
        timelock_contract_address: Address,
        l1_multicall3_address: Address,
        state_transition_chain_contract: Address,
        rollup_chain_id: L2ChainId,
        custom_commit_sender_addr: Option<Address>,
    ) -> Self {
        let eth_client = eth_client.for_component("eth_tx_aggregator");
        let functions = ZkSyncFunctions::default();
        let base_nonce = eth_client.pending_nonce().await.unwrap().as_u64();

        let base_nonce_custom_commit_sender = match custom_commit_sender_addr {
            Some(addr) => Some(
                (*eth_client)
                    .as_ref()
                    .nonce_at_for_account(addr, BlockNumber::Pending)
                    .await
                    .unwrap()
                    .as_u64(),
            ),
            None => None,
        };

        let metrics = Arc::new(DataAvailabilityMetrics::default());

        let worker_config = WorkerConfig {
            ipfs_retry_base_delay: Duration::from_secs(1),
            ipfs_retry_max_delay: Duration::from_secs(60),
            ipfs_max_attempts: 5,
            mintlayer_retry_base: Duration::from_secs(1),
            mintlayer_retry_max_delay: Duration::from_secs(60),
            mintlayer_max_attempts: 5,
            cleanup_interval: Duration::from_secs(300),
            cleanup_days_threshold: 7,
            batch_size: 6,
        };

        let data_availability_worker =
            DataAvailabilityWorker::new(worker_config, pool.clone(), metrics.clone());
        let mut eth_tx_aggregator = Self {
            config,
            aggregator,
            eth_client,
            timelock_contract_address,
            l1_multicall3_address,
            state_transition_chain_contract,
            functions,
            base_nonce,
            base_nonce_custom_commit_sender,
            rollup_chain_id,
            custom_commit_sender_addr,
            pool,
            data_availability_worker: Some(data_availability_worker),
            metrics,
        };

        if let Some(worker) = eth_tx_aggregator.data_availability_worker.take() {
            tokio::spawn(async move {
                worker.run().await;
            });
        }

        eth_tx_aggregator
    }

    pub async fn run(mut self, stop_receiver: watch::Receiver<bool>) -> anyhow::Result<()> {
        let pool = self.pool.clone();
        loop {
            let mut storage = pool.connection_tagged("eth_sender").await.unwrap();

            if *stop_receiver.borrow() {
                tracing::info!("Stop signal received, eth_tx_aggregator is shutting down");
                break;
            }

            if let Err(err) = self.loop_iteration(&mut storage).await {
                // Web3 API request failures can cause this,
                // and anything more important is already properly reported.
                tracing::warn!("eth_sender error {err:?}");
            }

            tokio::time::sleep(self.config.aggregate_tx_poll_period()).await;
        }
        Ok(())
    }

    pub(super) async fn get_multicall_data(&self) -> Result<MulticallData, EthSenderError> {
        let calldata = self.generate_calldata_for_multicall();
        let args = CallFunctionArgs::new(&self.functions.aggregate3.name, calldata).for_contract(
            self.l1_multicall3_address,
            &self.functions.multicall_contract,
        );
        let aggregate3_result: Token = args.call((*self.eth_client).as_ref()).await?;
        self.parse_multicall_data(aggregate3_result)
    }

    // Multicall's aggregate function accepts 1 argument - arrays of different contract calls.
    // The role of the method below is to tokenize input for multicall, which is actually a vector of tokens.
    // Each token describes a specific contract call.
    pub(super) fn generate_calldata_for_multicall(&self) -> Vec<Token> {
        const ALLOW_FAILURE: bool = false;

        // First zksync contract call
        let get_l2_bootloader_hash_input = self
            .functions
            .get_l2_bootloader_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_bootloader_hash_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_bootloader_hash_input,
        };

        // Second zksync contract call
        let get_l2_default_aa_hash_input = self
            .functions
            .get_l2_default_account_bytecode_hash
            .encode_input(&[])
            .unwrap();
        let get_default_aa_hash_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_l2_default_aa_hash_input,
        };

        // Third zksync contract call
        let get_verifier_params_input = self
            .functions
            .get_verifier_params
            .encode_input(&[])
            .unwrap();
        let get_verifier_params_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_params_input,
        };

        // Fourth zksync contract call
        let get_verifier_input = self.functions.get_verifier.encode_input(&[]).unwrap();
        let get_verifier_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_verifier_input,
        };

        // Fifth zksync contract call
        let get_protocol_version_input = self
            .functions
            .get_protocol_version
            .encode_input(&[])
            .unwrap();
        let get_protocol_version_call = Multicall3Call {
            target: self.state_transition_chain_contract,
            allow_failure: ALLOW_FAILURE,
            calldata: get_protocol_version_input,
        };

        // Convert structs into tokens and return vector with them
        vec![
            get_bootloader_hash_call.into_token(),
            get_default_aa_hash_call.into_token(),
            get_verifier_params_call.into_token(),
            get_verifier_call.into_token(),
            get_protocol_version_call.into_token(),
        ]
    }

    // The role of the method below is to de-tokenize multicall call's result, which is actually a token.
    // This token is an array of tuples like `(bool, bytes)`, that contain the status and result for each contract call.
    pub(super) fn parse_multicall_data(
        &self,
        token: Token,
    ) -> Result<MulticallData, EthSenderError> {
        let parse_error = |tokens: &[Token]| {
            Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                format!("Failed to parse multicall token: {:?}", tokens),
            )))
        };

        if let Token::Array(call_results) = token {
            // 5 calls are aggregated in multicall
            if call_results.len() != 5 {
                return parse_error(&call_results);
            }
            let mut call_results_iterator = call_results.into_iter();

            let multicall3_bootloader =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;

            if multicall3_bootloader.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 bootloader hash data is not of the len of 32: {:?}",
                        multicall3_bootloader
                    ),
                )));
            }
            let bootloader = H256::from_slice(&multicall3_bootloader);

            let multicall3_default_aa =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_default_aa.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 default aa hash data is not of the len of 32: {:?}",
                        multicall3_default_aa
                    ),
                )));
            }
            let default_aa = H256::from_slice(&multicall3_default_aa);
            let base_system_contracts_hashes = BaseSystemContractsHashes {
                bootloader,
                default_aa,
            };

            let multicall3_verifier_params =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_verifier_params.len() != 96 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 verifier params data is not of the len of 96: {:?}",
                        multicall3_default_aa
                    ),
                )));
            }
            let recursion_node_level_vk_hash = H256::from_slice(&multicall3_verifier_params[..32]);
            let recursion_leaf_level_vk_hash =
                H256::from_slice(&multicall3_verifier_params[32..64]);
            let recursion_circuits_set_vks_hash =
                H256::from_slice(&multicall3_verifier_params[64..]);
            let verifier_params = VerifierParams {
                recursion_node_level_vk_hash,
                recursion_leaf_level_vk_hash,
                recursion_circuits_set_vks_hash,
            };

            let multicall3_verifier_address =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_verifier_address.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 verifier address data is not of the len of 32: {:?}",
                        multicall3_verifier_address
                    ),
                )));
            }
            let verifier_address = Address::from_slice(&multicall3_verifier_address[12..]);

            let multicall3_protocol_version =
                Multicall3Result::from_token(call_results_iterator.next().unwrap())?.return_data;
            if multicall3_protocol_version.len() != 32 {
                return Err(EthSenderError::Parse(Web3ContractError::InvalidOutputType(
                    format!(
                        "multicall3 protocol version data is not of the len of 32: {:?}",
                        multicall3_protocol_version
                    ),
                )));
            }

            let protocol_version = U256::from_big_endian(&multicall3_protocol_version);
            // In case the protocol version is smaller than `PACKED_SEMVER_MINOR_MASK`, it will mean that it is
            // equal to the `protocol_version_id` value, since it the interface from before the semver was supported.
            let protocol_version_id = if protocol_version < U256::from(PACKED_SEMVER_MINOR_MASK) {
                ProtocolVersionId::try_from(protocol_version.as_u32() as u16).unwrap()
            } else {
                ProtocolVersionId::try_from_packed_semver(protocol_version).unwrap()
            };

            return Ok(MulticallData {
                base_system_contracts_hashes,
                verifier_params,
                verifier_address,
                protocol_version_id,
            });
        }
        parse_error(&[token])
    }

    /// Loads current verifier config on L1
    async fn get_recursion_scheduler_level_vk_hash(
        &mut self,
        verifier_address: Address,
    ) -> Result<H256, EthSenderError> {
        let get_vk_hash = &self.functions.verification_key_hash;
        let vk_hash: H256 = CallFunctionArgs::new(&get_vk_hash.name, ())
            .for_contract(verifier_address, &self.functions.verifier_contract)
            .call((*self.eth_client).as_ref())
            .await?;
        Ok(vk_hash)
    }

    #[tracing::instrument(skip(self, storage))]
    async fn loop_iteration(
        &mut self,
        storage: &mut Connection<'_, Core>,
    ) -> Result<(), EthSenderError> {
        let MulticallData {
            base_system_contracts_hashes,
            verifier_params,
            verifier_address,
            protocol_version_id,
        } = self.get_multicall_data().await.map_err(|err| {
            tracing::error!("Failed to get multicall data {err:?}");
            err
        })?;
        let contracts_are_pre_shared_bridge = protocol_version_id.is_pre_shared_bridge();

        let recursion_scheduler_level_vk_hash = self
            .get_recursion_scheduler_level_vk_hash(verifier_address)
            .await
            .map_err(|err| {
                tracing::error!("Failed to get VK hash from the Verifier {err:?}");
                err
            })?;
        let l1_verifier_config = L1VerifierConfig {
            params: verifier_params,
            recursion_scheduler_level_vk_hash,
        };

        if let Some(agg_op) = self
            .aggregator
            .get_next_ready_operation(
                storage,
                base_system_contracts_hashes,
                protocol_version_id,
                l1_verifier_config,
            )
            .await
        {
            let tx = self
                .save_eth_tx(storage, &agg_op, contracts_are_pre_shared_bridge)
                .await?;
            Self::report_eth_tx_saving(storage, &agg_op, &tx).await;

            // zkthunder: A method `save_mintlayer_tx` to send the op to ipfs and mintlayer.
            self.save_mintlayer_tx(&agg_op).await;
        }

        Ok(())
    }

    async fn save_mintlayer_tx(&mut self, aggregated_op: &AggregatedOperation) {
        let pending_op = PendingIpfsOperation {
            id: uuid::Uuid::new_v4(),
            operation_type: OperationType::from_str(
                &aggregated_op.get_action_caption().to_string(),
            )
            .map_err(|e| DataAvailabilityError::DatabaseError(e.to_string()))
            .unwrap(),
            data: serde_json::to_vec(aggregated_op).unwrap(),
            attempts: 0,
            last_attempt: None,
            created_at: Utc::now(),
            status: OperationStatus::Pending,
            ipfs_hash: None,
            requires_mintlayer: true,
        };

        let mut conn = match self.pool.connection_tagged("eth_tx_aggregator").await {
            Ok(conn) => conn,
            Err(e) => {
                tracing::error!("Failed to connect to database: {}", e);
                return;
            }
        };

        if let Err(e) = conn
            .data_availability_dal()
            .save_pending_operation(&pending_op)
            .await
        {
            tracing::error!("Failed to save pending operation: {}", e);
            return;
        }

        tracing::info!(
            "Saved operation for later processing: {} ({:?})",
            pending_op.id,
            pending_op.operation_type
        )
    }

    async fn report_eth_tx_saving(
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        tx: &EthTx,
    ) {
        let l1_batch_number_range = aggregated_op.l1_batch_range();
        tracing::info!(
            "eth_tx with ID {} for op {} was saved for L1 batches {l1_batch_number_range:?}",
            tx.id,
            aggregated_op.get_action_caption()
        );

        if let AggregatedOperation::Commit(_, l1_batches, _) = aggregated_op {
            for batch in l1_batches {
                METRICS.pubdata_size[&PubdataKind::StateDiffs]
                    .observe(batch.metadata.state_diffs_compressed.len());
                METRICS.pubdata_size[&PubdataKind::UserL2ToL1Logs]
                    .observe(batch.header.l2_to_l1_logs.len() * UserL2ToL1Log::SERIALIZED_SIZE);
                METRICS.pubdata_size[&PubdataKind::LongL2ToL1Messages]
                    .observe(batch.header.l2_to_l1_messages.iter().map(Vec::len).sum());
                METRICS.pubdata_size[&PubdataKind::RawPublishedBytecodes]
                    .observe(batch.raw_published_factory_deps.iter().map(Vec::len).sum());
            }
        }

        let range_size = l1_batch_number_range.end().0 - l1_batch_number_range.start().0 + 1;
        METRICS.block_range_size[&aggregated_op.get_action_type().into()]
            .observe(range_size.into());
        METRICS
            .track_eth_tx_metrics(storage, BlockL1Stage::Saved, tx)
            .await;
    }

    fn encode_aggregated_op(
        &self,
        op: &AggregatedOperation,
        contracts_are_pre_shared_bridge: bool,
    ) -> TxData {
        let operation_is_pre_shared_bridge = op.protocol_version().is_pre_shared_bridge();

        // The post shared bridge contracts support pre-shared bridge operations, but vice versa is not true.
        if contracts_are_pre_shared_bridge {
            assert!(operation_is_pre_shared_bridge);
        }

        let mut args = vec![Token::Uint(self.rollup_chain_id.as_u64().into())];

        let (calldata, sidecar) = match op {
            AggregatedOperation::Commit(last_committed_l1_batch, l1_batches, pubdata_da) => {
                let commit_batches = CommitBatches {
                    last_committed_l1_batch,
                    l1_batches,
                    pubdata_da: *pubdata_da,
                    mode: self.aggregator.mode(),
                };
                let commit_data_base = commit_batches.into_tokens();

                let (encoding_fn, commit_data) = if contracts_are_pre_shared_bridge {
                    (&self.functions.pre_shared_bridge_commit, commit_data_base)
                } else {
                    args.extend(commit_data_base);
                    (
                        self.functions
                            .post_shared_bridge_commit
                            .as_ref()
                            .expect("Missing ABI for commitBatchesSharedBridge"),
                        args,
                    )
                };

                let l1_batch_for_sidecar = if PubdataDA::Blobs == self.aggregator.pubdata_da() {
                    Some(l1_batches[0].clone())
                } else {
                    None
                };

                Self::encode_commit_data(encoding_fn, &commit_data, l1_batch_for_sidecar)
            }
            AggregatedOperation::PublishProofOnchain(op) => {
                let calldata = if contracts_are_pre_shared_bridge {
                    self.functions
                        .pre_shared_bridge_prove
                        .encode_input(&op.into_tokens())
                        .expect("Failed to encode prove transaction data")
                } else {
                    args.extend(op.into_tokens());
                    self.functions
                        .post_shared_bridge_prove
                        .as_ref()
                        .expect("Missing ABI for proveBatchesSharedBridge")
                        .encode_input(&args)
                        .expect("Failed to encode prove transaction data")
                };
                (calldata, None)
            }
            AggregatedOperation::Execute(op) => {
                let calldata = if contracts_are_pre_shared_bridge {
                    self.functions
                        .pre_shared_bridge_execute
                        .encode_input(&op.into_tokens())
                        .expect("Failed to encode execute transaction data")
                } else {
                    args.extend(op.into_tokens());
                    self.functions
                        .post_shared_bridge_execute
                        .as_ref()
                        .expect("Missing ABI for executeBatchesSharedBridge")
                        .encode_input(&args)
                        .expect("Failed to encode execute transaction data")
                };
                (calldata, None)
            }
        };
        TxData { calldata, sidecar }
    }

    fn encode_commit_data(
        commit_fn: &Function,
        commit_payload: &[Token],
        l1_batch: Option<L1BatchWithMetadata>,
    ) -> (Vec<u8>, Option<EthTxBlobSidecar>) {
        let calldata = commit_fn
            .encode_input(commit_payload)
            .expect("Failed to encode commit transaction data");

        let sidecar = match l1_batch {
            None => None,
            Some(l1_batch) => {
                let sidecar = l1_batch
                    .header
                    .pubdata_input
                    .clone()
                    .unwrap()
                    .chunks(ZK_SYNC_BYTES_PER_BLOB)
                    .map(|blob| {
                        let kzg_info = KzgInfo::new(blob);
                        SidecarBlobV1 {
                            blob: kzg_info.blob.to_vec(),
                            commitment: kzg_info.kzg_commitment.to_vec(),
                            proof: kzg_info.blob_proof.to_vec(),
                            versioned_hash: kzg_info.versioned_hash.to_vec(),
                        }
                    })
                    .collect::<Vec<SidecarBlobV1>>();

                let eth_tx_blob_sidecar = EthTxBlobSidecarV1 { blobs: sidecar };
                Some(eth_tx_blob_sidecar.into())
            }
        };

        (calldata, sidecar)
    }

    pub(super) async fn save_eth_tx(
        &self,
        storage: &mut Connection<'_, Core>,
        aggregated_op: &AggregatedOperation,
        contracts_are_pre_shared_bridge: bool,
    ) -> Result<EthTx, EthSenderError> {
        let mut transaction = storage.start_transaction().await.unwrap();
        let op_type = aggregated_op.get_action_type();
        // We may be using a custom sender for commit transactions, so use this
        // var whatever it actually is: a `None` for single-addr operator or `Some`
        // for multi-addr operator in 4844 mode.
        let sender_addr = match op_type {
            AggregatedActionType::Commit => self.custom_commit_sender_addr,
            _ => None,
        };
        let nonce = self.get_next_nonce(&mut transaction, sender_addr).await?;
        let encoded_aggregated_op =
            self.encode_aggregated_op(aggregated_op, contracts_are_pre_shared_bridge);
        let l1_batch_number_range = aggregated_op.l1_batch_range();

        let predicted_gas_for_batches = transaction
            .blocks_dal()
            .get_l1_batches_predicted_gas(l1_batch_number_range.clone(), op_type)
            .await
            .unwrap();
        let eth_tx_predicted_gas = agg_l1_batch_base_cost(op_type) + predicted_gas_for_batches;

        let eth_tx = transaction
            .eth_sender_dal()
            .save_eth_tx(
                nonce,
                encoded_aggregated_op.calldata,
                op_type,
                self.timelock_contract_address,
                eth_tx_predicted_gas,
                sender_addr,
                encoded_aggregated_op.sidecar,
            )
            .await
            .unwrap();

        transaction
            .blocks_dal()
            .set_eth_tx_id(l1_batch_number_range, eth_tx.id, op_type)
            .await
            .unwrap();
        transaction.commit().await.unwrap();
        Ok(eth_tx)
    }

    async fn get_next_nonce(
        &self,
        storage: &mut Connection<'_, Core>,
        from_addr: Option<Address>,
    ) -> Result<u64, EthSenderError> {
        let db_nonce = storage
            .eth_sender_dal()
            .get_next_nonce(from_addr)
            .await
            .unwrap()
            .unwrap_or(0);
        // Between server starts we can execute some txs using operator account or remove some txs from the database
        // At the start we have to consider this fact and get the max nonce.
        Ok(if from_addr.is_none() {
            db_nonce.max(self.base_nonce)
        } else {
            db_nonce.max(
                self.base_nonce_custom_commit_sender
                    .expect("custom base nonce is expected to be initialized; qed"),
            )
        })
    }
}
