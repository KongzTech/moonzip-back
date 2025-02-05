use crate::{ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::{prelude::*, system_program};

pub const FEE_PREFIX: &[u8] = b"fee";

pub fn fee_address() -> Pubkey {
    let (address, _) = Pubkey::find_program_address(&[FEE_PREFIX], &crate::ID);
    address
}

pub fn set_fee_config(ctx: Context<SetFeeConfigAccounts>, config: FeeConfig) -> Result<()> {
    ctx.accounts.fee.set_inner(FeeAccount {
        config,
        bump: ctx.bumps.fee,
    });
    Ok(())
}

pub fn extract_fee(ctx: Context<ExtractFeeAccounts>, data: ExtractFeeData) -> Result<()> {
    ctx.accounts.fee.sub_lamports(data.amount)?;
    ctx.accounts.receiver.add_lamports(data.amount)?;
    Ok(())
}

pub fn take_account_as_fee(ctx: Context<TakeAccountAsFeeAccounts>) -> Result<()> {
    ctx.accounts
        .fee
        .add_lamports(ctx.accounts.donor.lamports())?;
    ctx.accounts
        .donor
        .sub_lamports(ctx.accounts.donor.lamports())?;
    Ok(())
}

pub fn take_fee<'a, 'info>(
    system_program: &'a Program<'info, System>,
    fee_account: &'a Account<'info, FeeAccount>,
    payer: &'a Signer<'info>,
    fee: u64,
) -> Result<()> {
    system_program::transfer(
        CpiContext::new(
            system_program.to_account_info(),
            system_program::Transfer {
                from: payer.to_account_info(),
                to: fee_account.to_account_info(),
            },
        ),
        fee,
    )?;
    Ok(())
}

#[derive(Accounts)]
pub struct SetFeeConfigAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        init_if_needed,
        payer = authority,
        space = FeeAccount::ACCOUNT_SIZE, seeds = [FEE_PREFIX], bump
    )]
    pub fee: Account<'info, FeeAccount>,

    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, PartialEq, PartialOrd, Debug)]
pub struct ExtractFeeData {
    pub amount: u64,
}

#[derive(Accounts)]
pub struct ExtractFeeAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut,
        seeds = [FEE_PREFIX], bump=fee.bump
    )]
    pub fee: Account<'info, FeeAccount>,

    /// CHECK: only for lamports receiving
    #[account(mut)]
    pub receiver: UncheckedAccount<'info>,
}

#[derive(Accounts)]
pub struct TakeAccountAsFeeAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut, seeds = [FEE_PREFIX], bump=fee.bump)]
    pub fee: Account<'info, FeeAccount>,

    /// CHECK: only for lamports taking
    #[account(mut)]
    pub donor: UncheckedAccount<'info>,
}

#[account]
#[derive(PartialEq, PartialOrd, Debug)]
pub struct FeeAccount {
    pub config: FeeConfig,
    pub bump: u8,
}

impl Sizable for FeeAccount {
    fn longest() -> Self {
        Self {
            config: Sizable::longest(),
            bump: Sizable::longest(),
        }
    }
}

ensure_account_size!(FeeAccount, 13);

#[derive(AnchorDeserialize, AnchorSerialize, Clone, PartialEq, PartialOrd, Debug)]
pub struct FeeConfig {
    pub on_buy: BasisPoints,
    pub on_sell: BasisPoints,
}

impl Sizable for FeeConfig {
    fn longest() -> Self {
        Self {
            on_buy: Sizable::longest(),
            on_sell: Sizable::longest(),
        }
    }
}

#[derive(AnchorDeserialize, AnchorSerialize, Clone, PartialEq, PartialOrd, Debug)]
pub struct BasisPoints(u16);

impl BasisPoints {
    const MAX: u16 = 10000;

    pub fn part_of(&self, amount: u64) -> u64 {
        ((amount as u128).saturating_mul(self.0 as u128) / (Self::MAX as u128)) as u64
    }

    pub fn on_top_of(&self, amount: u64) -> u64 {
        let opposite_bps = Self::MAX - self.0;
        (amount.saturating_mul(Self::MAX as u64) / (opposite_bps as u64)).saturating_sub(amount)
    }
}

impl Sizable for BasisPoints {
    fn longest() -> Self {
        Self(Sizable::longest())
    }
}
