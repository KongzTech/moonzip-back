use crate::{ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, MintTo, Token, TokenAccount},
};
use curve::{BuyCalculator, CurveState, SellCalculator};
use global::{GlobalCurvedPoolAccount, GLOBAL_ACCOUNT_PREFIX};

pub mod curve;
pub mod global;

pub const CURVED_POOL_PREFIX: &[u8] = b"curved-pool";
pub const DEFAULT_MIN_TRADEABLE_SOL: u64 = 1_000;

pub fn create(ctx: Context<CreateCurvedPoolAccounts>, data: CreateCurvedPoolData) -> Result<()> {
    anchor_spl::token::mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        ctx.accounts.global.config.curve.total_token_supply,
    )?;
    let curve = CurveState::from_cfg(&ctx.accounts.global.config.curve);

    ctx.accounts.pool.set_inner(CurvedPool {
        mint: ctx.accounts.mint.key(),
        config: data.config,
        curve,
        status: CurvedPoolStatus::Active,
        bump: ctx.bumps.pool,
    });

    Ok(())
}

pub fn graduate(ctx: Context<GraduateCurvedPoolAccounts>) -> Result<()> {
    if !matches!(ctx.accounts.pool.status, CurvedPoolStatus::Closed) {
        return err!(CurvedPoolError::NotClosed);
    }

    let pool = ctx
        .accounts
        .pool
        .sub_lamports(ctx.accounts.pool.curve.sol_balance())?;
    ctx.accounts
        .funds_receiver
        .add_lamports(ctx.accounts.pool.curve.sol_balance())?;
    pool.close(ctx.accounts.authority.to_account_info())?;

    Ok(())
}

pub fn buy(ctx: Context<BuyFromCurvedPoolAccounts>, data: BuyFromCurvedPoolData) -> Result<()> {
    let sols = BuyCalculator::new(&ctx.accounts.pool.curve).fixed_tokens(data.tokens);
    if sols > data.max_sol_cost {
        return err!(CurvedPoolError::SlippageFailure);
    }
    if sols < ctx.accounts.pool.config.min_tradeable_sol() {
        return err!(CurvedPoolError::SolLimitReached);
    }

    ctx.accounts.pool.curve.commit_buy(sols, data.tokens);
    ctx.accounts.pool.close_if_needed();
    let bump = &[ctx.accounts.pool.bump][..];

    anchor_spl::token::transfer(
        CpiContext::new_with_signer(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: ctx.accounts.pool_token_account.to_account_info(),
                to: ctx.accounts.user_token_account.to_account_info(),
                authority: ctx.accounts.pool.to_account_info(),
            },
            &[&[CURVED_POOL_PREFIX, ctx.accounts.mint.key().as_ref(), bump]],
        ),
        data.tokens,
    )?;

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        sols,
    )?;

    Ok(())
}

pub fn sell(ctx: Context<SellFromCurvedPoolAccounts>, data: SellFromCurvedPoolData) -> Result<()> {
    let sols = SellCalculator::new(&ctx.accounts.pool.curve).fixed_tokens(data.tokens);
    if sols < data.min_sol_output {
        return err!(CurvedPoolError::SlippageFailure);
    }
    if sols < ctx.accounts.pool.config.min_tradeable_sol() {
        return err!(CurvedPoolError::SolLimitReached);
    }
    ctx.accounts.pool.curve.commit_sell(data.tokens, sols);

    anchor_spl::token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_token_account.to_account_info(),
                to: ctx.accounts.pool_token_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        data.tokens,
    )?;

    ctx.accounts.pool.sub_lamports(sols)?;
    ctx.accounts.user.add_lamports(sols)?;

    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Default, Clone, PartialEq, PartialOrd)]
pub struct CurvedPoolConfig {
    min_tradeable_sol: Option<u64>,
    max_lamports: u64,
}

impl CurvedPoolConfig {
    pub fn min_tradeable_sol(&self) -> u64 {
        self.min_tradeable_sol.unwrap_or(DEFAULT_MIN_TRADEABLE_SOL)
    }
}

impl Sizable for CurvedPoolConfig {
    fn longest() -> Self {
        Self {
            min_tradeable_sol: Some(Sizable::longest()),
            max_lamports: Sizable::longest(),
        }
    }
}

#[account]
#[derive(Default, PartialEq, PartialOrd)]
pub struct CurvedPool {
    pub mint: Pubkey,
    pub config: CurvedPoolConfig,
    pub curve: CurveState,
    pub status: CurvedPoolStatus,
    pub bump: u8,
}

impl CurvedPool {
    pub fn close_if_needed(&mut self) {
        if self.curve.sol_balance() >= self.config.max_lamports {
            self.status = CurvedPoolStatus::Closed;
        }
    }
}

impl Sizable for CurvedPool {
    fn longest() -> Self {
        Self {
            mint: Default::default(),
            config: Sizable::longest(),
            status: Sizable::longest(),
            curve: Sizable::longest(),
            bump: Sizable::longest(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, AnchorSerialize, AnchorDeserialize)]
pub enum CurvedPoolStatus {
    Active,
    Closed,
}

impl Default for CurvedPoolStatus {
    fn default() -> Self {
        Self::Active
    }
}

impl Sizable for CurvedPoolStatus {
    fn longest() -> Self {
        Self::Active
    }
}

ensure_account_size!(CurvedPool, 99);

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateCurvedPoolData {
    config: CurvedPoolConfig,
}

#[derive(Accounts)]
pub struct CreateCurvedPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        seeds = [GLOBAL_ACCOUNT_PREFIX], bump=global.bump
    )]
    pub global: Account<'info, GlobalCurvedPoolAccount>,

    #[account(
        init,
        payer = authority,
        mint::decimals = 9,
        mint::authority = authority,
        mint::freeze_authority = authority
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_token_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        space = CurvedPool::ACCOUNT_SIZE, seeds = [CURVED_POOL_PREFIX, mint.key().as_ref()], bump
    )]
    pub pool: Account<'info, CurvedPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct BuyFromCurvedPoolData {
    pub tokens: u64,
    pub max_sol_cost: u64,
}

#[derive(Accounts)]
pub struct BuyFromCurvedPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_token_account: Account<'info, TokenAccount>,

    #[account(mut,
        seeds = [CURVED_POOL_PREFIX, mint.key().as_ref()], bump=pool.bump
    )]
    pub pool: Account<'info, CurvedPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct SellFromCurvedPoolData {
    pub tokens: u64,
    pub min_sol_output: u64,
}

#[derive(Accounts)]
pub struct SellFromCurvedPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub user: Signer<'info>,
    #[account(constraint = pool.mint == mint.key())]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub user_pool_account: Account<'info, TokenAccount>,

    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub user_token_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        seeds = [CURVED_POOL_PREFIX, mint.key().as_ref()], bump = pool.bump
    )]
    pub pool: Account<'info, CurvedPool>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_token_account: Account<'info, TokenAccount>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct GraduateCurvedPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    /// CHECK: only for lamports receiving
    #[account(mut)]
    pub funds_receiver: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [CURVED_POOL_PREFIX, pool.mint.as_ref()], bump = pool.bump
    )]
    pub pool: Account<'info, CurvedPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[error_code]
pub enum CurvedPoolError {
    #[msg("Slippage failure: curve represents higher price")]
    SlippageFailure,

    #[msg("Transaction amount is limited due to pool config, limit is violated")]
    SolLimitReached,

    #[msg("If close date is specified, it must be future")]
    CloseDateBehindClock,

    #[msg("Boundary conditions must be set for the pool - either close date, or max lamports, or both..")]
    BoundaryConditionsNotSet,

    #[msg("Pool is not closed yet")]
    NotClosed,

    #[msg("Pool is not graduated yet")]
    NotGraduated,
}
