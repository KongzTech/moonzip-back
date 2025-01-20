use backend::{
    api::router,
    app::{migrator::Migrator, storage::StorageClient, App},
    cfg,
};
use services_common::{
    api::server::{serve, AppState},
    cfg::load_config,
    solana::pool::SolanaPool,
};
use std::sync::Arc;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cfg = load_config::<crate::cfg::Config>();
    let storage_client = StorageClient::from_config(cfg.db).await?;
    let solana_pool = SolanaPool::from_cfg(cfg.solana_pool)?;

    Migrator::serve(solana_pool.clone(), storage_client.clone(), cfg.migrator);

    let app = Arc::new(App::new(storage_client, solana_pool).await);
    let api_state = AppState::new(app, cfg.api);

    serve(api_state, router()).await?;
    Ok(())
}
