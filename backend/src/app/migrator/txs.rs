use crate::app::instructions::solana;
use anyhow::bail;
use serde::{Deserialize, Serialize};
use services_common::{
    solana::{any_tx::AnyTx, pool::SolanaPool},
    utils::period_fetch::DataReceiver,
};
use solana_sdk::{
    commitment_config::{CommitmentConfig, CommitmentLevel},
    hash::Hash,
    instruction::Instruction,
    signature::{Keypair, Signature},
    signer::Signer,
    transaction::Transaction,
};
use std::{sync::Arc, time::Duration};
use tokio::time::Instant;
use tracing::{debug, info, instrument, warn};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TxExecutorConfig {
    pub max_tries: u32,
    #[serde(with = "humantime_serde")]
    pub err_retry_interval: Duration,
}

pub fn default_err_retry_interval() -> Duration {
    Duration::from_millis(200)
}

pub struct TxExecutor {
    solana_pool: SolanaPool,
    solana_meta: DataReceiver<solana::Meta>,
    config: Arc<TxExecutorConfig>,
}

impl TxExecutor {
    pub fn new(
        solana_pool: SolanaPool,
        solana_meta: DataReceiver<solana::Meta>,
        config: TxExecutorConfig,
    ) -> Self {
        Self {
            solana_pool,
            solana_meta,
            config: Arc::new(config),
        }
    }

    pub async fn execute_single(&self, request: TransactionRequest) -> anyhow::Result<()> {
        let mut solana_meta = self.solana_meta.clone();
        let mut tries = 0;
        while tries <= self.config.max_tries {
            tries += 1;
            let result = self.execute_single_tick(&mut solana_meta, &request).await;
            match result {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(err)) => {
                    bail!("fatal error, stop execution: {err:?}");
                }
                Err(err) => {
                    warn!("transaction submission failed, retrying: {err:?}");
                    tokio::time::sleep(self.config.err_retry_interval).await;
                }
            }
        }
        bail!(
            "transaction submission failed after {} tries",
            self.config.max_tries
        );
    }

    async fn execute_single_tick(
        &self,
        meta: &mut DataReceiver<solana::Meta>,
        request: &TransactionRequest,
    ) -> anyhow::Result<anyhow::Result<()>> {
        let blockhash = meta.get()?.recent_blockhash;
        let tx = request.signed(blockhash);
        let signature = self.solana_pool.jito_client().submit_single_tx(&tx).await?;
        self.wait_by_signature(&signature).await?;

        Ok(Ok(()))
    }

    #[instrument(skip(self))]
    async fn wait_by_signature(&self, signature: &Signature) -> anyhow::Result<()> {
        let wait_commitment = CommitmentConfig::confirmed();

        let max_wait = Duration::from_millis(1500);
        let sleep_on_missing = Duration::from_millis(500);
        let before = Instant::now();

        while before.elapsed() < max_wait {
            let result = self
                .solana_pool
                .rpc_client()
                .use_single()
                .await
                .get_signature_status_with_commitment(signature, wait_commitment)
                .await?;
            let Some(result) = result else {
                debug!("transaction signature not found: validator ignored");
                tokio::time::sleep(sleep_on_missing).await;
                continue;
            };
            if let Err(err) = result {
                return Err(anyhow::anyhow!("transaction returned error: {err:?}"));
            }

            info!("transaction confirmed successfully");
            return Ok(());
        }

        bail!("timeout elapsed: {max_wait:?}")
    }

    pub async fn execute_batch(&self, requests: Vec<TransactionRequest>) -> anyhow::Result<()> {
        let mut solana_meta = self.solana_meta.clone();
        let mut tries = 0;
        while tries <= self.config.max_tries {
            tries += 1;
            let result = self.execute_batch_tick(&mut solana_meta, &requests).await;
            match result {
                Ok(Ok(())) => return Ok(()),
                Ok(Err(err)) => {
                    bail!("fatal error, stop execution: {err:?}");
                }
                Err(err) => {
                    warn!("transaction batch submission failed, retrying: {err:?}");
                    tokio::time::sleep(self.config.err_retry_interval).await;
                }
            }
        }
        bail!(
            "transaction batch submission failed after {} tries",
            self.config.max_tries
        );
    }

    async fn execute_batch_tick(
        &self,
        meta: &mut DataReceiver<solana::Meta>,
        requests: &[TransactionRequest],
    ) -> anyhow::Result<anyhow::Result<()>> {
        let blockhash = meta.get()?.recent_blockhash;

        let txs = requests
            .iter()
            .map(|r| r.signed(blockhash))
            .collect::<Vec<_>>();

        let bundle_id = self.solana_pool.jito_client().submit_bundle(txs).await?;
        self.watch_by_bundle_id(bundle_id).await?;

        Ok(Ok(()))
    }

    #[instrument(skip(self))]
    async fn watch_by_bundle_id(&self, bundle_id: String) -> anyhow::Result<()> {
        let wait_commitment = CommitmentLevel::Confirmed;
        let confirm_timeout = Duration::from_secs(2);

        let before = Instant::now();

        while before.elapsed() < confirm_timeout {
            let status = self
                .solana_pool
                .jito_client()
                .get_bundle_status(&bundle_id)
                .await?;
            if status.confirmation_status == wait_commitment {
                info!("bundle completed successfully");
                return Ok(());
            }
            if let Err(err) = status.err {
                bail!("transaction batch resulted in error: {err:?}");
            }
            tokio::time::sleep(Duration::from_millis(300)).await;
        }
        bail!("bundle await timeout")
    }
}

pub struct TransactionRequest {
    pub instructions: Vec<Instruction>,
    pub signers: Vec<Keypair>,
    pub payer: Keypair,
}

impl TransactionRequest {
    fn signed(&self, recent_blockhash: Hash) -> AnyTx {
        let mut tx = Transaction::new_with_payer(&self.instructions, Some(&self.payer.pubkey()));
        tx.sign(&self.signers.iter().collect::<Vec<_>>(), recent_blockhash);
        AnyTx::from(tx)
    }
}
