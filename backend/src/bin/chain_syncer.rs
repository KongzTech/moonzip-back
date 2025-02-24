use backend::{
    app::{
        chain_sync::{
            cfg::ChainSyncConfig,
            fetcher::ChainFetcher,
            geyser::{GeyserClient, GeyserClientConfig},
            parser::ParseAggregator,
            storage::StorageApplier,
        },
        storage::{StorageClient, StorageConfig},
    },
    log::setup_log,
};
use serde::Deserialize;
use services_common::cfg::load_config;

#[derive(Deserialize, Debug, Clone)]
struct Config {
    db: StorageConfig,
    geyser: GeyserClientConfig,
    #[serde(default)]
    algo: ChainSyncConfig,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    setup_log();
    let cfg = load_config::<Config>();
    let storage_client = StorageClient::from_config(cfg.db).await?;
    let geyser = GeyserClient::from_cfg(cfg.geyser).await?;

    let blocks_rx = ChainFetcher::new(geyser).serve();
    let parsed_blocks_rx = ParseAggregator::new(blocks_rx, cfg.algo).serve();

    StorageApplier::new(storage_client, parsed_blocks_rx)
        .serve()
        .await?;
    panic!("storage applier unexpectedly terminated")
}
