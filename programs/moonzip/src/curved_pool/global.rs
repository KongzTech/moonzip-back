use super::{curve::CurveConfig, CurvedPoolConfig};
use crate::{ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::prelude::*;

pub const GLOBAL_ACCOUNT_PREFIX: &[u8] = b"curved-pool-global-account";

pub fn set_global_config(
    ctx: Context<SetCurvedPoolGlobalConfigAccounts>,
    config: GlobalCurvedPoolConfig,
) -> Result<()> {
    ctx.accounts.global.set_inner(GlobalCurvedPoolAccount {
        config,
        bump: ctx.bumps.global,
    });
    Ok(())
}

#[derive(Accounts)]
pub struct SetCurvedPoolGlobalConfigAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        init_if_needed,
        payer = authority,
        space = GlobalCurvedPoolAccount::ACCOUNT_SIZE, seeds = [GLOBAL_ACCOUNT_PREFIX], bump
    )]
    pub global: Account<'info, GlobalCurvedPoolAccount>,

    pub system_program: Program<'info, System>,
}

#[account]
#[derive(Default, PartialEq, PartialOrd, Debug)]
pub struct GlobalCurvedPoolAccount {
    pub config: GlobalCurvedPoolConfig,
    pub bump: u8,
}

impl Sizable for GlobalCurvedPoolAccount {
    fn longest() -> Self {
        Self {
            config: Sizable::longest(),
            bump: Sizable::longest(),
        }
    }
}

ensure_account_size!(GlobalCurvedPoolAccount, 60);

#[derive(AnchorDeserialize, AnchorSerialize, Clone, PartialEq, PartialOrd, Debug)]
pub struct GlobalCurvedPoolConfig {
    pub curve: CurveConfig,
    pub token_decimals: u8,
    pub pool: CurvedPoolConfig,
}

impl Default for GlobalCurvedPoolConfig {
    fn default() -> Self {
        Self {
            curve: Default::default(),
            token_decimals: 6,
            pool: Default::default(),
        }
    }
}

impl Sizable for GlobalCurvedPoolConfig {
    fn longest() -> Self {
        Self {
            curve: Sizable::longest(),
            token_decimals: Sizable::longest(),
            pool: Sizable::longest(),
        }
    }
}
