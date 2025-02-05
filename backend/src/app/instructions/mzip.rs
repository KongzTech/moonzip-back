use anchor_client::anchor_lang::AccountDeserialize as _;
use anyhow::Context as _;
use moonzip::moonzip::GlobalCurvedPoolAccount;
use once_cell::sync::Lazy;
use services_common::{solana::pool::SolanaPool, utils::period_fetch::FetchExecutor};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

pub struct MetaFetcher {
    pub pool: SolanaPool,
}

#[async_trait::async_trait]
impl FetchExecutor<Meta> for MetaFetcher {
    fn name(&self) -> &'static str {
        "mzip-meta"
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn fetch(&mut self) -> anyhow::Result<Meta> {
        let client = self.pool.rpc_client().use_single().await;
        let global = client
            .get_account_with_commitment(&GLOBAL_ACCOUNT, CommitmentConfig::finalized())
            .await?;
        let marker = global.context.slot;
        let data = global
            .value
            .ok_or_else(|| anyhow::anyhow!("no global curve pool account yet"))?
            .data;
        let global_account = GlobalCurvedPoolAccount::try_deserialize(&mut data.as_slice())
            .with_context(|| format!("deserialize global curve pool account, raw: {data:?}"))?;
        Ok(Meta {
            marker,
            global_account,
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct Meta {
    pub marker: u64,
    pub global_account: GlobalCurvedPoolAccount,
}

impl PartialOrd for Meta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.marker.partial_cmp(&other.marker)
    }
}

pub static GLOBAL_ACCOUNT: Lazy<Pubkey> = Lazy::new(global_account_address);

fn global_account_address() -> Pubkey {
    Pubkey::find_program_address(&[b"curved-pool-global-account"], &moonzip::ID).0
}
