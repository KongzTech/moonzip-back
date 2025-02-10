use crate::{
    ensure_account_size,
    fee::{take_fee, FeeAccount, FEE_ACCOUNT_PREFIX},
    utils::Sizable,
    Project, ProjectId, ProjectStage, PROGRAM_AUTHORITY, PROJECT_PREFIX,
};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, MintTo, Token, TokenAccount},
};
use curve::{BuyCalculator, CalcBuy as _, CalcSell as _, CurveState, SellCalculator};
use global::{GlobalCurvedPoolAccount, GLOBAL_ACCOUNT_PREFIX};

pub mod curve;
pub mod global;

pub const CURVED_POOL_PREFIX: &[u8] = b"curved-pool";
pub const DEFAULT_MIN_TRADEABLE_SOL: u64 = 1_000;

pub fn curved_pool_address(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[CURVED_POOL_PREFIX, mint.as_ref()], &crate::ID).0
}

pub fn create(ctx: Context<CreateCurvedPoolAccounts>, data: CreateCurvedPoolData) -> Result<()> {
    ctx.accounts.project.ensure_can_create_curved_pool()?;

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
        config: ctx.accounts.global.config.pool,
        curve,
        status: CurvedPoolStatus::Active,
        project_id: data.project_id,
        bump: ctx.bumps.pool,
    });

    ctx.accounts.project.stage = ProjectStage::CurvePoolActive;

    Ok(())
}

pub fn graduate(ctx: Context<GraduateCurvedPoolAccounts>) -> Result<()> {
    if !ctx.accounts.pool.close_if_needed() {
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
    if ctx.accounts.pool.status == CurvedPoolStatus::Closed {
        return err!(CurvedPoolError::AlreadyClosed);
    }
    let sols = data.sols;
    let fee = ctx.accounts.fee.config.on_buy.part_of(sols);
    let after_fee = sols.saturating_sub(fee);

    let tokens = BuyCalculator::new(&ctx.accounts.pool.curve).fixed_sols(after_fee);

    if tokens < data.min_token_output {
        return err!(CurvedPoolError::SlippageFailure);
    }
    if !ctx.accounts.pool.buy_allowed(after_fee, tokens) {
        return err!(CurvedPoolError::OperationDisallowed);
    }

    ctx.accounts.pool.curve.commit_buy(after_fee, tokens);

    if ctx.accounts.pool.close_if_needed() {
        ctx.accounts.project.stage = ProjectStage::CurvePoolClosed;
    }

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
        tokens,
    )?;

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        after_fee,
    )?;

    take_fee(
        &ctx.accounts.system_program,
        &ctx.accounts.fee,
        &ctx.accounts.user,
        fee,
    )?;

    Ok(())
}

pub fn sell(ctx: Context<SellFromCurvedPoolAccounts>, data: SellFromCurvedPoolData) -> Result<()> {
    if ctx.accounts.pool.status == CurvedPoolStatus::Closed {
        return err!(CurvedPoolError::AlreadyClosed);
    }

    let sols = SellCalculator::new(&ctx.accounts.pool.curve).fixed_tokens(data.tokens);
    if sols < data.min_sol_output {
        return err!(CurvedPoolError::SlippageFailure);
    }
    if !ctx.accounts.pool.sell_allowed(data.tokens, sols) {
        return err!(CurvedPoolError::OperationDisallowed);
    }
    ctx.accounts.pool.curve.commit_sell(data.tokens, sols);

    if ctx.accounts.pool.close_if_needed() {
        ctx.accounts.project.stage = ProjectStage::CurvePoolClosed;
    }

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

    let fee = ctx.accounts.fee.config.on_sell.part_of(sols);
    ctx.accounts.pool.sub_lamports(sols)?;
    ctx.accounts.user.add_lamports(sols - fee)?;
    ctx.accounts.fee.add_lamports(fee)?;

    Ok(())
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Default, Clone, PartialEq, PartialOrd, Copy, Debug,
)]
pub struct CurvedPoolConfig {
    pub min_tradeable_sol: Option<u64>,
    pub min_sol_to_close: Option<u64>,
}

impl CurvedPoolConfig {
    pub fn min_tradeable_sol(&self) -> u64 {
        self.min_tradeable_sol.unwrap_or(DEFAULT_MIN_TRADEABLE_SOL)
    }

    pub fn min_sol_to_close(&self) -> u64 {
        self.min_sol_to_close.unwrap_or(0)
    }
}

impl Sizable for CurvedPoolConfig {
    fn longest() -> Self {
        Self {
            min_tradeable_sol: Some(Sizable::longest()),
            min_sol_to_close: Some(Sizable::longest()),
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
    pub project_id: ProjectId,
    pub bump: u8,
}

impl CurvedPool {
    pub fn close_if_needed(&mut self) -> bool {
        if self.curve.token_balance() <= self.tokens_to_close() {
            self.status = CurvedPoolStatus::Closed;
            true
        } else {
            false
        }
    }

    pub fn buy_allowed(&self, sols: u64, tokens: u64) -> bool {
        if tokens > self.curve.token_balance() {
            msg!("buy not allowed, tokens exceed curve's balance");
            return false;
        }

        // order limit must be disabled for the last tokens
        if tokens <= self.curve.token_balance() {
            return true;
        }

        sols >= self.config.min_tradeable_sol() && tokens > 0
    }

    pub fn sell_allowed(&self, tokens: u64, sols: u64) -> bool {
        sols > 0 && tokens > 0
    }

    fn tokens_to_close(&self) -> u64 {
        let calculator = BuyCalculator::new(&self.curve);
        calculator.fixed_sols(self.config.min_sol_to_close())
    }
}

impl Sizable for CurvedPool {
    fn longest() -> Self {
        Self {
            mint: Default::default(),
            config: Sizable::longest(),
            status: Sizable::longest(),
            curve: Sizable::longest(),
            project_id: Sizable::longest(),
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

ensure_account_size!(CurvedPool, 116);

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateCurvedPoolData {
    pub project_id: ProjectId,
}

#[derive(Accounts)]
#[instruction(data: CreateCurvedPoolData)]
pub struct CreateCurvedPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [PROJECT_PREFIX, &data.project_id.to_bytes()], bump = project.bump
    )]
    pub project: Account<'info, Project>,

    #[account(
        seeds = [GLOBAL_ACCOUNT_PREFIX], bump=global.bump
    )]
    pub global: Account<'info, GlobalCurvedPoolAccount>,

    #[account(
        init,
        payer = authority,
        mint::decimals = global.config.token_decimals,
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
    pub project_id: ProjectId,
    pub sols: u64,
    pub min_token_output: u64,
}

#[derive(Accounts)]
#[instruction(data: BuyFromCurvedPoolData)]
pub struct BuyFromCurvedPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [FEE_ACCOUNT_PREFIX], bump=fee.bump
    )]
    pub fee: Account<'info, FeeAccount>,

    #[account(
        mut,
        constraint = project.id == pool.project_id,
        seeds = [PROJECT_PREFIX, &data.project_id.to_bytes()], bump = project.bump
    )]
    pub project: Account<'info, Project>,

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
    pub project_id: ProjectId,
    pub tokens: u64,
    pub min_sol_output: u64,
}

#[derive(Accounts)]
#[instruction(data: SellFromCurvedPoolData)]
pub struct SellFromCurvedPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        seeds = [FEE_ACCOUNT_PREFIX], bump=fee.bump
    )]
    pub fee: Account<'info, FeeAccount>,

    #[account(
        mut,
        constraint = project.id == pool.project_id,
        seeds = [PROJECT_PREFIX, &data.project_id.to_bytes()], bump = project.bump
    )]
    pub project: Account<'info, Project>,

    #[account(mut)]
    pub user: Signer<'info>,
    #[account(constraint = pool.mint == mint.key())]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
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

    #[account(
        mut,
        seeds = [FEE_ACCOUNT_PREFIX], bump=fee.bump
    )]
    pub fee: Account<'info, FeeAccount>,

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
    #[msg("Pool is already closed")]
    AlreadyClosed,

    #[msg("Pool already exists for given project")]
    AlreadyCreated,

    #[msg("Slippage failure: curve represents higher price")]
    SlippageFailure,

    #[msg("Operation is disallowed due to pool config, some limit is violated")]
    OperationDisallowed,

    #[msg("If close date is specified, it must be future")]
    CloseDateBehindClock,

    #[msg("Boundary conditions must be set for the pool - either close date, or max lamports, or both..")]
    BoundaryConditionsNotSet,

    #[msg("Pool is not closed yet")]
    NotClosed,

    #[msg("Pool is not graduated yet")]
    NotGraduated,
}
