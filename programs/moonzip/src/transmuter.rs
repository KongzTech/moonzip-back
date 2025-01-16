use crate::{
    curved_pool::{
        curve::{CurveState, SellCalculator},
        CurvedPool, CURVED_POOL_PREFIX,
    },
    ensure_account_size,
    pumpfun::{self, seeds::BONDING_CURVE_SEED, CurveWrapper},
    utils::Sizable,
    PROGRAM_AUTHORITY,
};
use anchor_lang::prelude::*;
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};
use pumpfun_cpi::BondingCurve;

pub const TRANSMUTER_PREFIX: &[u8] = b"transmuter";

pub fn create(ctx: Context<CreateTransmuterAccounts>) -> Result<()> {
    ctx.accounts.transmuter.set_inner(Transmuter {
        from_mint: ctx.accounts.from_mint.key(),
        to_mint: ctx.accounts.to_mint.key(),
        status: TransmuterStatus::Created,
        bump: ctx.bumps.transmuter,
    });

    Ok(())
}

pub fn init_for_curve(ctx: Context<InitTransmuterForCurveAccounts>) -> Result<()> {
    let transmuter = &mut ctx.accounts.base.transmuter;
    transmuter.status = TransmuterStatus::Initialized {
        method: TransmuteMethod::CurveLimit {
            curve_snapshot: ctx.accounts.curved_pool.curve,
        },
    };

    Ok(())
}

pub fn init_for_pumpfun_curve(ctx: Context<InitTransmuterForPumpfunCurveAccounts>) -> Result<()> {
    let transmuter = &mut ctx.accounts.base.transmuter;
    transmuter.status = TransmuterStatus::Initialized {
        method: TransmuteMethod::PumpfunCurveLimit {
            curve_snapshot: (*ctx.accounts.bonding_curve).into(),
        },
    };

    Ok(())
}

pub fn transmute(ctx: Context<TransmuteAccounts>, data: TransmuteData) -> Result<()> {
    let TransmuterStatus::Initialized { method } = &ctx.accounts.transmuter.status else {
        return Err(TransmuterError::TransmuterInactive.into());
    };

    anchor_spl::token::burn(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Burn {
                from: ctx.accounts.user_from_token_account.to_account_info(),
                mint: ctx.accounts.from_mint.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        data.tokens,
    )?;

    if ctx.accounts.user_from_token_account.amount == 0 {
        anchor_spl::token::close_account(CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::CloseAccount {
                account: ctx.accounts.user_from_token_account.to_account_info(),
                destination: ctx.accounts.user.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ))?;
    }

    let tokens = match method {
        TransmuteMethod::CurveLimit { curve_snapshot } => {
            SellCalculator::new(curve_snapshot).fixed_sols(data.tokens)
        }
        TransmuteMethod::PumpfunCurveLimit { curve_snapshot } => {
            pumpfun::SellCalculator::new(curve_snapshot).fixed_sols(data.tokens)
        }
    };

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: ctx.accounts.transmuter_to_token_account.to_account_info(),
                to: ctx.accounts.user_to_token_account.to_account_info(),
                authority: ctx.accounts.transmuter.to_account_info(),
            },
            &[&[
                TRANSMUTER_PREFIX,
                ctx.accounts.transmuter.from_mint.as_ref(),
                ctx.accounts.transmuter.to_mint.as_ref(),
                &[ctx.accounts.transmuter.bump],
            ]],
        ),
        tokens,
    )?;

    Ok(())
}

#[derive(Accounts)]
pub struct CreateTransmuterAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    pub from_mint: Account<'info, Mint>,
    pub to_mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = to_mint,
        associated_token::authority = transmuter,
    )]
    pub to_mint_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        space = Transmuter::ACCOUNT_SIZE, seeds = [TRANSMUTER_PREFIX, from_mint.key().as_ref(), to_mint.key().as_ref()], bump
    )]
    pub transmuter: Account<'info, Transmuter>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct InitTransmuterForCurveAccounts<'info> {
    pub base: BaseInitTransmuterAccounts<'info>,

    #[account(
        seeds = [CURVED_POOL_PREFIX, base.to_mint.key().as_ref()], bump=curved_pool.bump
    )]
    pub curved_pool: Account<'info, CurvedPool>,
}

#[derive(Accounts)]
pub struct InitTransmuterForPumpfunCurveAccounts<'info> {
    pub base: BaseInitTransmuterAccounts<'info>,

    #[account(
        seeds = [BONDING_CURVE_SEED, base.to_mint.key().as_ref()], bump,
        seeds::program = pumpfun_cpi::ID
    )]
    pub bonding_curve: Account<'info, BondingCurve>,
}

#[derive(Accounts)]
pub struct BaseInitTransmuterAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    pub from_mint: Account<'info, Mint>,
    pub to_mint: Account<'info, Mint>,

    #[account(
        associated_token::mint = to_mint,
        associated_token::authority = transmuter,
    )]
    pub to_mint_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [TRANSMUTER_PREFIX, from_mint.key().as_ref(), to_mint.key().as_ref()], bump=transmuter.bump
    )]
    pub transmuter: Account<'info, Transmuter>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct TransmuteData {
    pub tokens: u64,
}

#[derive(Accounts)]
pub struct TransmuteAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub from_mint: Account<'info, Mint>,
    pub to_mint: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = from_mint,
        associated_token::authority = user,
    )]
    pub user_from_token_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = to_mint,
        associated_token::authority = user,
    )]
    pub user_to_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = to_mint,
        associated_token::authority = transmuter,
    )]
    pub transmuter_to_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [TRANSMUTER_PREFIX, from_mint.key().as_ref(), to_mint.key().as_ref()], bump=transmuter.bump
    )]
    pub transmuter: Account<'info, Transmuter>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[account]
#[derive(Debug)]
pub struct Transmuter {
    from_mint: Pubkey,
    to_mint: Pubkey,
    status: TransmuterStatus,
    bump: u8,
}

ensure_account_size!(Transmuter, 115);

impl Sizable for Transmuter {
    fn longest() -> Self {
        Self {
            from_mint: Pubkey::default(),
            to_mint: Pubkey::default(),
            status: Sizable::longest(),
            bump: Sizable::longest(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum TransmuterStatus {
    Created,
    Initialized { method: TransmuteMethod },
}

impl Sizable for TransmuterStatus {
    fn longest() -> Self {
        Self::Initialized {
            method: Sizable::longest(),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum TransmuteMethod {
    /// In this case, transmuter will give amount of tokens,
    /// which in case of sell would be sold for the amount of given `from_mint` tokens.
    ///
    /// It is therefore logical that `from_mint` is 1:1 with lamports in this case.
    ///
    /// It is intended to limit users so that they won't get immediate profit on sell at the curve start
    CurveLimit { curve_snapshot: CurveState },

    /// Same as [ Self::CurveLimit ], but for pumpfun curve.
    PumpfunCurveLimit { curve_snapshot: CurveWrapper },
}

impl Sizable for TransmuteMethod {
    fn longest() -> Self {
        Self::CurveLimit {
            curve_snapshot: Sizable::longest(),
        }
    }
}

#[error_code]
pub enum TransmuterError {
    #[msg("Transmuter is not yet activated")]
    TransmuterInactive,
}
