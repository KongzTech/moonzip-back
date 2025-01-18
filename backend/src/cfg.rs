use serde::Deserialize;

use crate::api;
use crate::app::state::StorageConfig;
use crate::solana::SolanaConfig;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    pub api: api::server::Config,
    pub db: StorageConfig,
    pub solana: SolanaConfig,
}

pub fn load() -> Config {
    let run_mode = std::env::var("APP_RUN_MODE").unwrap_or_else(|_| "dev".into());

    config::Config::builder()
        .add_source(config::File::with_name("config/default").required(false))
        .add_source(config::File::with_name(&format!("config/{}", run_mode)).required(false))
        .add_source(config::File::with_name("config/local").required(false))
        .add_source(
            config::Environment::default()
                .prefix("APP")
                .prefix_separator("_")
                .separator("__"),
        )
        .build()
        .unwrap()
        .try_deserialize::<Config>()
        .unwrap()
}
