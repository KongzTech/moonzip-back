use backend::{
    app::{
        instructions::{self, mzip, pumpfun, InstructionsBuilder, InstructionsConfig},
        keys_loader::{self, KeysLoader},
        migrator::{Migrator, MigratorConfig},
        storage::{StorageClient, StorageConfig},
    },
    cfg::FetchersConfig,
    log::setup_log,
    solana::{SolanaKeys, SolanaKeysConfig},
};
use serde::Deserialize;
use services_common::{
    cfg::load_config,
    solana::pool::{SolanaPool, SolanaPoolConfig},
    utils::period_fetch::{PeriodicFetcher, PeriodicFetcherConfig},
};

#[derive(Deserialize, Debug, Clone)]
struct Config {
    db: StorageConfig,
    keys: SolanaKeysConfig,
    solana_pool: SolanaPoolConfig,
    migrator: MigratorConfig,
    token_keys_loader: keys_loader::Config,
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
    KeysLoader::new(cfg.token_keys_loader, storage_client.clone()).serve();
    let keys = SolanaKeys::from_cfg(cfg.keys);

    let solana_meta = PeriodicFetcher::new(
        instructions::solana::MetaFetcher::new(solana_pool.clone()),
        cfg.fetchers.solana_meta,
    )
    .serve();
    let pump_meta = PeriodicFetcher::new(
        pumpfun::MetaFetcher {
            pool: solana_pool.clone(),
        },
        PeriodicFetcherConfig::every_hour(),
    )
    .serve();
    let mzip_meta = PeriodicFetcher::new(
        mzip::MetaFetcher {
            pool: solana_pool.clone(),
        },
        PeriodicFetcherConfig::every_hour(),
    )
    .serve();

    let instructions_builder = InstructionsBuilder {
        solana_pool: solana_pool.clone(),
        solana_meta: solana_meta.clone(),
        pump_meta,
        mzip_meta,
        config: cfg.instructions.into(),
    };

    let handle = Migrator::serve(
        solana_pool.clone(),
        keys.clone(),
        storage_client.clone(),
        instructions_builder.clone(),
        cfg.migrator,
    )
    .await?;
    handle.await?;
    Ok(())
}
