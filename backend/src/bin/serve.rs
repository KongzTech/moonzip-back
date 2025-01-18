use backend::{
    api::server::{serve, AppState},
    app::{state::StorageClient, App},
    cfg,
    solana::SolanaPool,
};
use std::sync::Arc;

#[tokio::main]
pub async fn main() -> anyhow::Result<()> {
    let cfg = cfg::load();
    let storage_client = StorageClient::from_config(cfg.db).await?;
    let solana_pool = SolanaPool::from_cfg(cfg.solana);
    let app = Arc::new(App::new(storage_client, solana_pool).await);
    let api_state = AppState::new(app, cfg.api);
    serve(api_state).await?;
    Ok(())
}
