use serde::Deserialize;
use services_common::utils::keypair::SaneKeypair;

#[derive(Debug, Deserialize, Clone)]
pub struct SolanaKeysConfig {
    pub authority: SaneKeypair,
}

pub struct SolanaKeys {
    pub authority: SaneKeypair,
}

impl SolanaKeys {
    pub fn from_cfg(cfg: SolanaKeysConfig) -> Self {
        Self {
            authority: cfg.authority,
        }
    }

    pub fn authority_keypair(&self) -> &SaneKeypair {
        &self.authority
    }
}
