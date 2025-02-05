use backend::{
    api::router,
    app::{
        instructions::{self, InstructionsBuilder, InstructionsConfig},
        storage::{StorageClient, StorageConfig},
        App,
    },
    cfg::FetchersConfig,
    log::setup_log,
    solana::{SolanaKeys, SolanaKeysConfig},
};
use serde::Deserialize;
use services_common::{
    api::server::{serve, ApiConfig, AppState},
    cfg::load_config,
    solana::pool::{SolanaPool, SolanaPoolConfig},
    utils::period_fetch::PeriodicFetcher,
};
use std::sync::Arc;
use tracing::info;

#[derive(Deserialize, Debug, Clone)]
struct Config {
    api: ApiConfig,
    db: StorageConfig,
    keys: SolanaKeysConfig,
    solana_pool: SolanaPoolConfig,
    #[serde(default)]
    instructions: InstructionsConfig,
    fetchers: FetchersConfig,
}

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    setup_log();
    let cfg = load_config::<Config>();
    let storage_client = StorageClient::from_config(cfg.db).await?;
    let solana_pool = SolanaPool::from_cfg(cfg.solana_pool)?;
    let keys = SolanaKeys::from_cfg(cfg.keys);

    let solana_meta = PeriodicFetcher::new(
        instructions::solana::MetaFetcher::new(solana_pool.clone()),
        cfg.fetchers.solana_meta,
    )
    .serve();

    let instructions_builder = InstructionsBuilder {
        solana_pool: solana_pool.clone(),
        solana_meta: solana_meta.clone(),
        config: cfg.instructions.into(),
    };

    let app = Arc::new(App {
        storage: storage_client,
        instructions_builder,
        keys,
        solana_meta,
    });
    let api_state = AppState::new(app, cfg.api);
    info!("Starting API server");
    serve::<_, backend::api::ApiDoc>(api_state, router()).await?;
    Ok(())
}
