use crate::{
    curved_pool::{curve::CurveState, CurvedPool, CURVED_POOL_PREFIX},
    ensure_account_size,
    pumpfun::{self, seeds::BONDING_CURVE_SEED, CurveWrapper},
    utils::Sizable,
    PROGRAM_AUTHORITY,
};
use anchor_lang::{prelude::*, Bumps};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, Token, TokenAccount},
};
use pumpfun_cpi::BondingCurve;

pub const TRANSMUTER_PREFIX: &[u8] = b"transmuter";

pub fn init_for_curve(ctx: Context<InitTransmuterForCurveAccounts>) -> Result<()> {
    let method = TransmuteMethod::CurveLimit {
        curve_snapshot: ctx.accounts.curved_pool.curve,
    };
    base_transmuter_init(ctx, method)?;

    Ok(())
}

pub fn init_for_pumpfun_curve(ctx: Context<InitTransmuterForPumpfunCurveAccounts>) -> Result<()> {
    let method = TransmuteMethod::PumpfunCurveLimit {
        curve_snapshot: (*ctx.accounts.bonding_curve).into(),
    };
    base_transmuter_init(ctx, method)?;

    Ok(())
}

fn base_transmuter_init<'a, A: TransmuterInitAccounts<'a> + Bumps>(
    ctx: Context<A>,
    method: TransmuteMethod,
) -> Result<()> {
    let bump = ctx.accounts.transmuter_bump(&ctx.bumps);
    let base = ctx.accounts.base();
    base.transmuter.set_inner(Transmuter {
        from_mint: base.from_mint.key(),
        to_mint: base.to_mint.key(),
        method,
        bump,
    });

    anchor_spl::token::transfer(
        CpiContext::new(
            base.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: base.donor_to_mint_account.to_account_info(),
                to: base.transmuter_to_mint_account.to_account_info(),
                authority: base.donor.to_account_info(),
            },
        ),
        base.donor_to_mint_account.amount,
    )?;

    anchor_spl::token::close_account(CpiContext::new(
        base.token_program.to_account_info(),
        anchor_spl::token::CloseAccount {
            account: base.donor_to_mint_account.to_account_info(),
            destination: base.authority.to_account_info(),
            authority: base.donor.to_account_info(),
        },
    ))?;
    Ok(())
}

pub fn transmute(ctx: Context<TransmuteAccounts>, data: TransmuteData) -> Result<()> {
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

    let tokens = match &ctx.accounts.transmuter.method {
        TransmuteMethod::CurveLimit { curve_snapshot } => {
            super::curve::SellCalculator::new(curve_snapshot).fixed_sols(data.tokens)
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

    if ctx.accounts.transmuter_to_token_account.amount == 0 {
        anchor_spl::token::close_account(CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::CloseAccount {
                account: ctx.accounts.transmuter_to_token_account.to_account_info(),
                destination: ctx.accounts.transmuter.to_account_info(),
                authority: ctx.accounts.transmuter.to_account_info(),
            },
        ))?;
    }

    ctx.accounts
        .transmuter
        .close(ctx.accounts.authority.to_account_info())?;

    Ok(())
}

trait TransmuterInitAccounts<'info>: Bumps {
    fn base(&mut self) -> &mut BaseInitTransmuterAccounts<'info>;
    fn transmuter_bump(&self, bumps: &<Self as Bumps>::Bumps) -> u8;
}

#[derive(Accounts)]
pub struct InitTransmuterForCurveAccounts<'info> {
    pub base: BaseInitTransmuterAccounts<'info>,

    #[account(
        seeds = [CURVED_POOL_PREFIX, base.to_mint.key().as_ref()], bump=curved_pool.bump
    )]
    pub curved_pool: Account<'info, CurvedPool>,
}

impl<'info> TransmuterInitAccounts<'info> for InitTransmuterForCurveAccounts<'info> {
    fn base(&mut self) -> &mut BaseInitTransmuterAccounts<'info> {
        &mut self.base
    }

    fn transmuter_bump(&self, bumps: &<Self as Bumps>::Bumps) -> u8 {
        bumps.base.transmuter
    }
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

impl<'info> TransmuterInitAccounts<'info> for InitTransmuterForPumpfunCurveAccounts<'info> {
    fn base(&mut self) -> &mut BaseInitTransmuterAccounts<'info> {
        &mut self.base
    }

    fn transmuter_bump(&self, bumps: &<Self as Bumps>::Bumps) -> u8 {
        bumps.base.transmuter
    }
}

#[derive(Accounts)]
pub struct BaseInitTransmuterAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    pub from_mint: Account<'info, Mint>,
    pub to_mint: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = to_mint,
        associated_token::authority = donor,
    )]
    pub donor_to_mint_account: Account<'info, TokenAccount>,
    pub donor: Signer<'info>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = to_mint,
        associated_token::authority = transmuter,
    )]
    pub transmuter_to_mint_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        space = Transmuter::ACCOUNT_SIZE,
        seeds = [TRANSMUTER_PREFIX, from_mint.key().as_ref(), to_mint.key().as_ref()], bump
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
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
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
    method: TransmuteMethod,
    bump: u8,
}

ensure_account_size!(Transmuter, 114);

impl Sizable for Transmuter {
    fn longest() -> Self {
        Self {
            from_mint: Pubkey::default(),
            to_mint: Pubkey::default(),
            method: Sizable::longest(),
            bump: Sizable::longest(),
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
