use std::{pin, time::Duration};

use anyhow::bail;
use futures::StreamExt;
use tokio::{
    spawn,
    sync::mpsc::{channel, Receiver, Sender},
    time::{sleep, Instant},
};
use tracing::error;
use yellowstone_grpc_proto::geyser::SubscribeRequestFilterTransactions;

use super::{geyser::GeyserClient, parser::ParseInput};

const BUFFER_CAPACITY: usize = 1000;
const MAX_SLEEP: Duration = Duration::from_millis(400);

pub struct ChainFetcher {
    geyser_client: GeyserClient,
    results_tx: Option<Sender<ParseInput>>,
}

impl ChainFetcher {
    pub fn new(geyser_client: GeyserClient) -> Self {
        Self {
            geyser_client,
            results_tx: None,
        }
    }

    pub fn serve(mut self) -> Receiver<ParseInput> {
        let (tx, rx) = channel(BUFFER_CAPACITY);
        self.results_tx = Some(tx);
        spawn(async move {
            loop {
                let before = Instant::now();
                if let Err(err) = self.tick().await {
                    error!("chain fetcher tick error: {err:#}")
                }
                sleep(MAX_SLEEP.saturating_sub(before.elapsed())).await;
            }
        });
        rx
    }

    async fn tick(&mut self) -> anyhow::Result<()> {
        let results_tx = self.results_tx.clone().expect("invariant: no results tx");
        let stream = self
            .geyser_client
            .subscribe_txs(SubscribeRequestFilterTransactions {
                vote: Some(false),
                failed: Some(false),
                signature: None,
                account_include: vec![
                    moonzip::ID_CONST.to_string(),
                    pumpfun_cpi::ID_CONST.to_string(),
                ],
                account_exclude: vec![],
                account_required: vec![],
            })
            .await?;
        let mut stream = pin::pin!(stream);
        while let Some(msg) = stream.next().await {
            let msg = msg?;
            let handle = || async {
                let transaction = msg
                    .transaction
                    .ok_or_else(|| anyhow::anyhow!("unexpected: no transaction in message"))?;
                let meta = transaction
                    .meta
                    .ok_or_else(|| anyhow::anyhow!("unexpected: no transaction meta in message"))?;
                let transaction = transaction
                    .transaction
                    .ok_or_else(|| anyhow::anyhow!("unexpected: no transaction"))?;
                let parse_input = ParseInput {
                    slot: msg.slot,
                    transaction,
                    meta,
                };
                results_tx.send(parse_input).await?;
                Result::<_, anyhow::Error>::Ok(())
            };
            if let Err(err) = handle().await {
                error!("failed to handle received message: {err:?}");
            }
        }
        bail!("stream must never terminate")
    }
}
