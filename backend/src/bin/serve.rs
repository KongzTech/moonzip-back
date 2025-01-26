use backend::{
    api::router,
    app::{
        instructions::{self, InstructionsBuilder},
        keys_loader::KeysLoader,
        migrator::Migrator,
        storage::StorageClient,
        App,
    },
    cfg,
    solana::SolanaKeys,
};
use services_common::{
    api::server::{serve, AppState},
    cfg::load_config,
    solana::pool::SolanaPool,
    utils::period_fetch::{PeriodicFetcher, PeriodicFetcherConfig},
};
use std::{sync::Arc, time::Duration};

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cfg = load_config::<crate::cfg::Config>();
    let storage_client = StorageClient::from_config(cfg.db).await?;
    let solana_pool = SolanaPool::from_cfg(cfg.solana_pool)?;
    KeysLoader::new(cfg.token_keys_loader, storage_client.clone()).serve();
    let keys = SolanaKeys::from_cfg(cfg.keys);

    let solana_meta = PeriodicFetcher::new(
        instructions::solana::MetaFetcher::new(solana_pool.clone()),
        PeriodicFetcherConfig {
            tick_interval: Duration::from_secs(1),
            error_backoff: Duration::from_secs(1),
        },
    )
    .serve();

    let instructions_builder = InstructionsBuilder {
        solana_pool: solana_pool.clone(),
        solana_meta: solana_meta.clone(),
        config: cfg.instructions.into(),
    };

    Migrator::serve(
        solana_pool.clone(),
        solana_meta.clone(),
        keys.clone(),
        storage_client.clone(),
        instructions_builder.clone(),
        cfg.migrator,
    )
    .await?;

    let app = Arc::new(App::new(storage_client, instructions_builder).await);
    let api_state = AppState::new(app, cfg.api);

    serve(api_state, router()).await?;
    Ok(())
}
