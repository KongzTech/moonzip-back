use super::curve::CurveConfig;
use crate::{ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::{prelude::*, solana_program::native_token::LAMPORTS_PER_SOL};

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
#[derive(Default, PartialEq, PartialOrd)]
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

ensure_account_size!(GlobalCurvedPoolAccount, 50);

#[derive(AnchorDeserialize, AnchorSerialize, Clone, PartialEq, PartialOrd)]
pub struct GlobalCurvedPoolConfig {
    pub curve: CurveConfig,
    pub token_decimals: u8,
    pub lamports_to_close: u64,
}

impl Default for GlobalCurvedPoolConfig {
    fn default() -> Self {
        Self {
            curve: Default::default(),
            token_decimals: 6,
            lamports_to_close: LAMPORTS_PER_SOL * 80,
        }
    }
}

impl Sizable for GlobalCurvedPoolConfig {
    fn longest() -> Self {
        Self {
            curve: Sizable::longest(),
            token_decimals: Sizable::longest(),
            lamports_to_close: Sizable::longest(),
        }
    }
}
