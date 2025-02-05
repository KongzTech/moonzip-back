use anchor_client::anchor_lang::AnchorDeserialize;
use once_cell::sync::Lazy;
use services_common::{solana::pool::SolanaPool, utils::period_fetch::FetchExecutor};
use solana_program::pubkey;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};

pub static MINT_AUTHORITY: Lazy<Pubkey> = Lazy::new(get_mint_authority);
pub static GLOBAL: Lazy<Pubkey> = Lazy::new(get_global);
pub const EVENT_AUTHORITY: Pubkey = pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");

fn get_mint_authority() -> Pubkey {
    Pubkey::find_program_address(&[b"mint-authority"], &pumpfun_cpi::ID).0
}

fn get_global() -> Pubkey {
    Pubkey::find_program_address(&[b"global"], &pumpfun_cpi::ID).0
}

pub fn get_bonding_curve(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], &pumpfun_cpi::ID).0
}

pub struct MetaFetcher {
    pub pool: SolanaPool,
}

#[async_trait::async_trait]
impl FetchExecutor<Meta> for MetaFetcher {
    fn name(&self) -> &'static str {
        "pumpfun-meta"
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn fetch(&mut self) -> anyhow::Result<Meta> {
        let client = self.pool.rpc_client().use_single().await;
        let global = client
            .get_account_with_commitment(&GLOBAL, CommitmentConfig::finalized())
            .await?;
        let marker = global.context.slot;
        Ok(Meta {
            marker,
            global_account: pumpfun_cpi::Global::try_from_slice(
                &global
                    .value
                    .ok_or_else(|| anyhow::anyhow!("no global curve pool account yet"))?
                    .data,
            )?,
        })
    }
}

#[derive(Clone)]
pub struct Meta {
    pub marker: u64,
    pub global_account: pumpfun_cpi::Global,
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
