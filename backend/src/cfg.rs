use serde::Deserialize;

use services_common::utils::period_fetch::PeriodicFetcherConfig;

#[derive(Deserialize, Debug, Clone)]
pub struct FetchersConfig {
    pub solana_meta: PeriodicFetcherConfig,
}
