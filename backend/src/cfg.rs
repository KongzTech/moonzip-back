use serde::Deserialize;

use crate::app::instructions::InstructionsConfig;
use crate::app::keys_loader;
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
    pub token_keys_loader: keys_loader::Config,
    pub instructions: InstructionsConfig,
}
