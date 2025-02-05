use derive_more::derive::Deref;
use serde::{Deserialize, Serialize};
use sqlx::query;

pub mod misc;
pub mod project;

pub type DB = sqlx::Postgres;
pub type DBTransaction<'a> = sqlx::Transaction<'a, DB>;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StorageConfig {
    pub url: String,
    #[serde(default = "default_max_connections")]
    pub max_connections: u32,
}

pub fn default_max_connections() -> u32 {
    5
}

#[derive(Deref, Clone)]
pub struct StorageClient {
    pub pool: sqlx::Pool<sqlx::Postgres>,
}

impl StorageClient {
    pub fn new(pool: sqlx::Pool<sqlx::Postgres>) -> Self {
        Self { pool }
    }

    pub async fn from_config(config: StorageConfig) -> anyhow::Result<Self> {
        let pool = sqlx::postgres::PgPoolOptions::new()
            .max_connections(config.max_connections)
            .connect(&config.url)
            .await?;
        Ok(Self::new(pool))
    }

    pub async fn serializable_tx(&self) -> anyhow::Result<sqlx::Transaction<'_, sqlx::Postgres>> {
        let mut tx = self.pool.begin().await?;
        query!("SET TRANSACTION ISOLATION LEVEL SERIALIZABLE")
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }
}
