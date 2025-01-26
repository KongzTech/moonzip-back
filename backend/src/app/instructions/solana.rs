use services_common::{solana::pool::SolanaPool, utils::period_fetch::FetchExecutor};
use solana_sdk::{commitment_config::CommitmentConfig, hash::Hash, sysvar::rent::Rent};

pub struct MetaFetcher {
    pub pool: SolanaPool,
    pub rent: Option<Rent>,
}

impl MetaFetcher {
    pub fn new(pool: SolanaPool) -> Self {
        Self { pool, rent: None }
    }
}

#[async_trait::async_trait]
impl FetchExecutor<Meta> for MetaFetcher {
    fn name(&self) -> &'static str {
        "solana-meta"
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        let client = self.pool.rpc_client().use_single().await;
        let rent_account = client.get_account(&solana_sdk::sysvar::rent::ID).await?;

        let data: Rent = rent_account.deserialize_data()?;
        self.rent = Some(data);

        Ok(())
    }

    async fn fetch(&mut self) -> anyhow::Result<Meta> {
        let (blockhash, marker) = self
            .pool
            .rpc_client()
            .use_single()
            .await
            .get_latest_blockhash_with_commitment(CommitmentConfig::confirmed())
            .await?;
        Ok(Meta {
            rent: self
                .rent
                .ok_or_else(|| anyhow::anyhow!("invariant: rent not initialized"))?,
            recent_blockhash: blockhash,
            marker,
        })
    }
}

#[derive(Clone)]
pub struct Meta {
    pub marker: u64,
    pub rent: Rent,
    pub recent_blockhash: Hash,
}

impl PartialEq for Meta {
    fn eq(&self, other: &Self) -> bool {
        self.marker == other.marker
    }
}

impl PartialOrd for Meta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.marker.partial_cmp(&other.marker)
    }
}
