use serde::Deserialize;

use crate::app::migrator::MigratorConfig;
use crate::app::storage::StorageConfig;
use crate::solana::SolanaKeysConfig;
use services_common::api::server::ApiConfig;
use services_common::solana::pool::SolanaPoolConfig;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub api: ApiConfig,
    pub db: StorageConfig,
    pub keys: SolanaKeysConfig,
    pub solana_pool: SolanaPoolConfig,
    pub migrator: MigratorConfig,
}
