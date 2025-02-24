use std::time::Duration;

use anyhow::{bail, Context};
use futures::{SinkExt as _, Stream, TryStreamExt};
use serde::Deserialize;
use yellowstone_grpc_client::{GeyserGrpcClient, InterceptorXToken};
use yellowstone_grpc_proto::geyser::{
    subscribe_update::UpdateOneof, CommitmentLevel, SubscribeRequest,
    SubscribeRequestFilterTransactions, SubscribeRequestPing, SubscribeUpdateTransaction,
};

#[derive(Deserialize, Debug, Clone)]
pub struct GeyserClientConfig {
    pub endpoint: String,
}

pub struct GeyserClient {
    client: GeyserGrpcClient<InterceptorXToken>,
}

impl GeyserClient {
    pub async fn from_cfg(cfg: GeyserClientConfig) -> anyhow::Result<Self> {
        Ok(Self {
            client: GeyserGrpcClient::build_from_shared(cfg.endpoint)?
                .timeout(Duration::from_secs(60))
                .connect()
                .await
                .context("connect to geyser server")?,
        })
    }

    pub async fn subscribe_txs(
        &mut self,
        filter: SubscribeRequestFilterTransactions,
    ) -> anyhow::Result<impl Stream<Item = anyhow::Result<SubscribeUpdateTransaction>>> {
        let request = SubscribeRequest {
            accounts: Default::default(),
            slots: Default::default(),
            transactions: [("client".to_string(), filter)].into_iter().collect(),
            transactions_status: Default::default(),
            blocks: Default::default(),
            blocks_meta: Default::default(),
            entry: Default::default(),
            commitment: Some(CommitmentLevel::Confirmed as i32),
            accounts_data_slice: Default::default(),
            ping: Default::default(),
        };

        let (subscribe_tx, stream) = self.client.subscribe_with_request(Some(request)).await?;

        Ok(stream
            .map_err(|err| anyhow::anyhow!("unexpected error occurred: {err:#?}"))
            .try_filter_map(move |update| {
                let mut tx = subscribe_tx.clone();
                async move {
                    match update.update_oneof {
                        Some(UpdateOneof::Transaction(update)) => Ok(Some(update)),
                        Some(UpdateOneof::Ping(_)) => {
                            // This is necessary to keep load balancers that expect client pings alive. If your load balancer doesn't
                            // require periodic client pings then this is unnecessary
                            tx.send(SubscribeRequest {
                                ping: Some(SubscribeRequestPing { id: 1 }),
                                ..Default::default()
                            })
                            .await?;
                            Ok(None)
                        }
                        Some(UpdateOneof::Pong(_)) => Ok(None),
                        msg => bail!("unexpected msg occurred: {msg:?}"),
                    }
                }
            }))
    }
}
