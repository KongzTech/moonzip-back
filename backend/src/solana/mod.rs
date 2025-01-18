use crate::utils::keypair::SaneKeypair;
use anchor_client::Client;
use serde::Deserialize;
use std::sync::Arc;

#[derive(Deserialize, Debug, Clone)]
pub struct SolanaConfig {
    pub authority_key: SaneKeypair,
}

#[derive(Clone)]
pub struct SolanaPool {
    config: Arc<SolanaConfig>,
}

impl SolanaPool {
    pub fn from_cfg(cfg: SolanaConfig) -> Self {
        Self { config: cfg.into() }
    }

    pub fn for_authority(&self) -> Client<SaneKeypair> {
        anchor_client::Client::new(
            anchor_client::Cluster::Debug,
            self.config.authority_key.clone(),
        )
    }
}
