use std::sync::Arc;

use super::any_tx::AnyTx;
use crate::utils::period_fetch::FetchExecutor;
use crate::utils::{decode_response_type_or_raw, decode_type_or_raw};
use anyhow::{bail, Context as _};
use futures_util::stream::SplitStream;
use futures_util::StreamExt as _;
use once_cell::sync::Lazy;
use rand::rngs::ThreadRng;
use rand::Rng as _;
use serde::Deserialize;
use serde_json::json;
use solana_sdk::commitment_config::CommitmentLevel;
use solana_sdk::signature::Signature;
use solana_sdk::{
    instruction::Instruction, native_token::sol_to_lamports, pubkey::Pubkey,
    system_instruction::transfer,
};
use tokio::net::TcpStream;
use tokio_tungstenite::{connect_async, MaybeTlsStream, WebSocketStream};

#[derive(Deserialize, Debug, Clone)]
pub struct JitoClientConfig {
    #[serde(default = "default_jito_base_url")]
    pub base_url: String,
}

fn default_jito_base_url() -> String {
    "https://frankfurt.mainnet.block-engine.jito.wtf".into()
}

pub struct JitoClient {
    client: reqwest::Client,
    config: Arc<JitoClientConfig>,
}

impl JitoClient {
    pub fn new(config: JitoClientConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: Arc::new(config),
        }
    }

    pub async fn submit_single_tx(&self, tx: &AnyTx) -> anyhow::Result<Signature> {
        let serialized = tx.serialize_base64()?;
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": id,
          "method": "sendTransaction",
          "params": [
            serialized,
            {
                "encoding": "base64",
            }
          ]
        }
            );
        let result: APIResponse<String> = decode_response_type_or_raw(
            self.client
                .post(format!("{}/api/v1/transactions", self.config.base_url))
                .json(&request)
                .send()
                .await?,
        )
        .await?;
        result.result.parse().context("decode signature")
    }

    pub async fn submit_bundle(&self, txs: Vec<AnyTx>) -> anyhow::Result<String> {
        let serialized = txs
            .iter()
            .map(|tx| tx.serialize_base64())
            .collect::<anyhow::Result<Vec<_>>>()?;
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": id,
          "method": "sendBundle",
          "params": [
            serialized,
            {
                "encoding": "base64",
            }
          ]
        }
        );
        let result: APIResponse<String> = decode_response_type_or_raw(
            self.client
                .post(format!("{}/api/v1/bundles", self.config.base_url))
                .json(&request)
                .send()
                .await?,
        )
        .await?;
        result.result.parse().context("decode signature")
    }

    pub async fn get_bundle_status(&self, bundle_id: &str) -> anyhow::Result<BundleStatus> {
        let id = uuid::Uuid::new_v4().to_string();
        let request = json!(
        {
          "jsonrpc": "2.0",
          "id": id,
          "method": "getBundleStatuses",
          "params": [bundle_id]
        }
        );
        let result: APIResponse<Option<InnerBundleStatusResponse>> = decode_response_type_or_raw(
            self.client
                .post(format!("{}/api/v1/getBundleStatuses", self.config.base_url))
                .json(&request)
                .send()
                .await?,
        )
        .await?;

        let Some(result) = result.result else {
            bail!("empty bundle statuses: not landed");
        };

        result
            .value
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("unexpected: empty bundle statuses"))
    }
}

#[derive(Deserialize)]
struct APIResponse<T> {
    result: T,
}

#[derive(Deserialize, Debug, Clone)]
struct InnerBundleStatusResponse {
    value: Vec<BundleStatus>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct BundleStatus {
    pub confirmation_status: CommitmentLevel,
    pub err: Result<Option<()>, String>,
}

#[derive(Debug)]
pub enum JitoTipStateFetcher {
    Created,
    Initialized(SplitStream<WebSocketStream<MaybeTlsStream<TcpStream>>>),
}

impl Default for JitoTipStateFetcher {
    fn default() -> Self {
        Self::Created
    }
}

#[async_trait::async_trait]
impl FetchExecutor<TipState> for JitoTipStateFetcher {
    async fn fetch(&mut self) -> anyhow::Result<TipState> {
        let Self::Initialized(read) = &mut self else {
            panic!("invariant: jito tip state fetcher failed to initialize");
        };
        let tip: Vec<TipState> = decode_type_or_raw(
            read.next()
                .await
                .ok_or_else(|| anyhow::anyhow!("jito tip state fetcher stream closed"))??
                .into_data(),
        )?;
        let tip = tip
            .into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("empty ws list received from jito"))?;
        Ok(tip)
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        let (stream, _) = connect_async("wss://bundles.jito.wtf/api/v1/bundles/tip_stream").await?;
        let (_, read) = stream.split();
        *self = JitoTipStateFetcher::Initialized(read);
        Ok(())
    }

    fn name(&self) -> &'static str {
        "jito_tip_state"
    }
}

#[derive(Deserialize, Debug, Clone, Copy, PartialEq)]
pub struct TipState {
    pub landed_tips_75th_percentile: f64,
}

impl PartialOrd for TipState {
    fn partial_cmp(&self, _other: &Self) -> Option<std::cmp::Ordering> {
        // we always accept new tip state
        Some(std::cmp::Ordering::Less)
    }
}

static JITO_KEYS: Lazy<Vec<Pubkey>> = Lazy::new(|| {
    [
        "96gYZGLnJYVFmbjzopPSU6QiEV5fGqZNyN9nmNhvrZU5",
        "HFqU5x63VTqvQss8hp11i4wVV8bD44PvwucfZ2bU7gRe",
        "Cw8CFyM9FkoMi7K7Crf6HNQqf4uEMzpKw6QNghXLvLkY",
        "ADaUMid9yfUytqMBgopwjb2DTLSokTSzL1zt6iGPaS49",
        "DfXygSm4jCyNCybVYYK6DwvWqjKee8pbDmJGcLWNDXjh",
        "ADuUkR4vqLUMWXxW9gh6D6L8pMSawimctcNZ5pGwDcEt",
        "DttWaMuVvTiduZRnguLF7jNxTgiMBZ1hyAumKUiL2KRL",
        "3AVi9Tg9Uo68tJfuvoKvqKNWKkC5wPdSSdeBnizKZ6jT",
    ]
    .into_iter()
    .map(|key| key.parse().unwrap())
    .collect()
});

fn jito_tip_account() -> Pubkey {
    JITO_KEYS[rand::thread_rng().gen_range(0..JITO_KEYS.len())]
}

impl TipState {
    pub fn tip_ix(&self, payer: &Pubkey) -> Instruction {
        transfer(payer, &jito_tip_account(), self.optimal_tip())
    }

    pub fn optimal_tip(&self) -> u64 {
        let mut random = ThreadRng::default();
        sol_to_lamports(self.landed_tips_75th_percentile.min(0.002))
            .saturating_add(random.gen_range(0..100))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::{
        period_fetch::{PeriodicFetcher, PeriodicFetcherConfig},
        tests::logger_setup,
    };

    #[tokio::test]
    #[ignore = "requires jito rpc, network access"]
    async fn test_jito_watcher() -> anyhow::Result<()> {
        logger_setup();

        let mut rx = PeriodicFetcher::new(
            JitoTipStateFetcher::default(),
            PeriodicFetcherConfig::zero(),
        )
        .serve();
        assert!(rx.wait().await?.landed_tips_75th_percentile > 0.0);
        Ok(())
    }
}
