use crate::app::storage::StorageClient;
use serde::{Deserialize, Serialize};
use solana_sdk::signature::Keypair;
use std::{path::PathBuf, sync::Arc};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub directory: PathBuf,
}

#[derive(Clone)]
pub struct KeysProvider {
    #[allow(unused)]
    storage: StorageClient,
    #[allow(unused)]
    config: Arc<Config>,
}

impl KeysProvider {
    pub fn new(storage: StorageClient, config: Config) -> Self {
        Self {
            storage,
            config: Arc::new(config),
        }
    }

    pub async fn next(&self) -> anyhow::Result<Keypair> {
        todo!()
    }
}
