use serde::Deserialize;

#[derive(Debug, Clone, Deserialize, serde_derive_default::Default)]
pub struct ChainSyncConfig {
    #[serde(default)]
    pub allowed_mint_suffix: Option<String>,
}
