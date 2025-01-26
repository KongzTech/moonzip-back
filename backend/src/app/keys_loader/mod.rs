use super::storage::misc::StoredKeypair;
use crate::app::storage::StorageClient;
use serde::{Deserialize, Serialize};
use solana_sdk::{signature::Keypair, signer::Signer};
use sqlx::query;
use std::{
    path::{Path, PathBuf},
    sync::Arc,
    time::Duration,
};
use tokio::fs;
use tracing::{info, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Config {
    pub directory: PathBuf,
    #[serde(with = "humantime_serde", default = "default_tick_interval")]
    pub tick_interval: Duration,
}

fn default_tick_interval() -> Duration {
    Duration::from_secs(30)
}

#[derive(Clone)]
pub struct KeysLoader {
    #[allow(unused)]
    config: Arc<Config>,
    storage_client: StorageClient,
}

impl KeysLoader {
    pub fn new(config: Config, storage_client: StorageClient) -> Self {
        Self {
            config: Arc::new(config),
            storage_client,
        }
    }

    pub fn serve(self) {
        tokio::spawn(async move {
            loop {
                if let Err(err) = self.tick().await {
                    tracing::error!("keys loader tick failed: {err:#}");
                }
                tokio::time::sleep(self.config.tick_interval).await;
            }
        });
    }

    async fn tick(&self) -> anyhow::Result<()> {
        let mut entries = fs::read_dir(&self.config.directory).await?;

        while let Some(entry) = entries.next_entry().await? {
            let path = entry.path();
            if path.is_file() {
                match self.load_keypair_from_file(&path).await {
                    Ok(keypair) => info!("Loaded token keypair with pubkey: {}", keypair.pubkey()),
                    Err(err) => warn!("Failed to decode keypair from: {:?}: {err:#}", path),
                }
            }
        }

        Ok(())
    }

    async fn load_keypair_from_file(&self, path: &Path) -> anyhow::Result<Keypair> {
        let data = fs::read(path).await?;
        let keypair = Keypair::from_bytes(&data)?;

        let stored = StoredKeypair::from_keypair(&keypair);
        query!(
            "INSERT INTO mzip_keypair VALUES ($1) ON CONFLICT DO NOTHING",
            stored as _
        )
        .execute(&self.storage_client.pool)
        .await?;
        fs::remove_file(path).await?;
        Ok(keypair)
    }
}
