use anchor_client::anchor_lang::AccountDeserialize as _;
use anyhow::Context as _;
use moonzip::{
    fee::{FeeAccount, FEE_ACCOUNT_PREFIX},
    moonzip::{GlobalCurvedPoolAccount, GLOBAL_ACCOUNT_PREFIX},
};
use once_cell::sync::Lazy;
use services_common::{solana::pool::SolanaPool, utils::period_fetch::FetchExecutor};
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

use super::utils::anchor_event_authority;

pub static MOONZIP_EVENT_AUTHORITY: Lazy<Pubkey> =
    Lazy::new(|| anchor_event_authority(&moonzip::ID));

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
            .get_multiple_accounts_with_commitment(
                &[*GLOBAL_ACCOUNT, *FEE_ACCOUNT],
                CommitmentConfig::finalized(),
            )
            .await?;
        let marker = global.context.slot;
        let mut data = global.value.into_iter();
        let global_account = data
            .next()
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("no global account"))?;
        let fee_account = data
            .next()
            .flatten()
            .ok_or_else(|| anyhow::anyhow!("no fee account"))?;
        let global_account =
            GlobalCurvedPoolAccount::try_deserialize(&mut global_account.data.as_slice())
                .with_context(|| {
                    format!(
                        "deserialize global curve pool account, raw: {:?}",
                        global_account.data
                    )
                })?;
        let fee_account = FeeAccount::try_deserialize(&mut fee_account.data.as_slice())
            .with_context(|| {
                format!(
                    "deserialize global curve pool account, raw: {:?}",
                    fee_account.data
                )
            })?;
        Ok(Meta {
            marker,
            global_account,
            fee_account,
        })
    }
}

#[derive(Clone, PartialEq)]
pub struct Meta {
    pub marker: u64,
    pub global_account: GlobalCurvedPoolAccount,
    pub fee_account: FeeAccount,
}

impl PartialOrd for Meta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.marker.partial_cmp(&other.marker)
    }
}

pub static GLOBAL_ACCOUNT: Lazy<Pubkey> = Lazy::new(global_account_address);
pub static FEE_ACCOUNT: Lazy<Pubkey> = Lazy::new(fee_account_address);

fn global_account_address() -> Pubkey {
    Pubkey::find_program_address(&[GLOBAL_ACCOUNT_PREFIX], &moonzip::ID).0
}

fn fee_account_address() -> Pubkey {
    Pubkey::find_program_address(&[FEE_ACCOUNT_PREFIX], &moonzip::ID).0
}
