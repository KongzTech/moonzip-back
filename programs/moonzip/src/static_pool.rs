use crate::{
    common::PoolCloseConditions,
    ensure_account_size,
    project::{ProjectId, PROJECT_PREFIX},
    utils::Sizable,
    Project, ProjectStage, PROGRAM_AUTHORITY,
};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, MintTo, Token, TokenAccount},
};

// matches native SOL decimals so to be 1:1 with it.
const POOL_TOKEN_DECIMALS: u8 = 9;
pub const STATIC_POOL_PREFIX: &[u8] = b"static-pool";

pub fn static_pool_address(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[STATIC_POOL_PREFIX, mint.as_ref()], &crate::ID).0
}

pub fn create(ctx: Context<CreateStaticPoolAccounts>, data: CreateStaticPoolData) -> Result<()> {
    ctx.accounts.project.ensure_can_create_static_pool()?;

    ctx.accounts.pool.set_inner(StaticPool {
        mint: ctx.accounts.mint.key(),
        config: data.config,
        collected_lamports: 0,
        state: StaticPoolState::Active,
        project_id: data.project_id,
        bump: ctx.bumps.pool,
    });

    ctx.accounts.project.stage = ProjectStage::StaticPoolActive;

    Ok(())
}

pub fn graduate(ctx: Context<GraduateStaticPoolAccounts>) -> Result<()> {
    if !ctx.accounts.pool.close_if_needed() {
        return err!(StaticPoolError::NotClosed);
    }

    let pool = ctx
        .accounts
        .pool
        .sub_lamports(ctx.accounts.pool.collected_lamports)?;
    ctx.accounts
        .funds_receiver
        .add_lamports(ctx.accounts.pool.collected_lamports)?;
    pool.close(ctx.accounts.authority.to_account_info())?;

    Ok(())
}

pub fn buy(ctx: Context<BuyFromStaticPoolAccounts>, data: BuyFromStaticPoolData) -> Result<()> {
    if ctx.accounts.pool.close_if_needed() {
        return err!(StaticPoolError::AlreadyClosed);
    }

    let already_collected = ctx.accounts.pool.collected_lamports;

    let amount = if let Some(max_lamports) = ctx.accounts.pool.config.close_conditions.max_lamports
    {
        max_lamports
            .checked_sub(already_collected)
            .expect("invariant: overpurchase for limiting pool")
            .min(data.amount)
    } else {
        data.amount
    };
    ctx.accounts.pool.collected_lamports = ctx
        .accounts
        .pool
        .collected_lamports
        .checked_add(amount)
        .expect("invariant: lamports amount is out of bounds");

    if ctx.accounts.pool.close_if_needed() {
        ctx.accounts.project.stage = ProjectStage::StaticPoolClosed;
    }

    let balance_to_mint = amount.saturating_sub(ctx.accounts.pool_mint_account.amount);
    if balance_to_mint > 0 {
        anchor_spl::token::mint_to(
            CpiContext::new(
                ctx.accounts.token_program.to_account_info(),
                MintTo {
                    mint: ctx.accounts.mint.to_account_info(),
                    to: ctx.accounts.user_mint_account.to_account_info(),
                    authority: ctx.accounts.authority.to_account_info(),
                },
            ),
            amount,
        )?;
    }

    let owe_amount = amount.saturating_sub(balance_to_mint);
    if owe_amount > 0 {
        anchor_spl::token::transfer(
            CpiContext::new_with_signer(
                ctx.accounts.token_program.to_account_info(),
                anchor_spl::token::Transfer {
                    from: ctx.accounts.pool_mint_account.to_account_info(),
                    to: ctx.accounts.user_mint_account.to_account_info(),
                    authority: ctx.accounts.pool.to_account_info(),
                },
                &[&[
                    STATIC_POOL_PREFIX,
                    ctx.accounts.mint.key().as_ref(),
                    &[ctx.accounts.pool.bump],
                ]],
            ),
            owe_amount,
        )?;
    }

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.user.to_account_info(),
                to: ctx.accounts.pool.to_account_info(),
            },
        ),
        amount,
    )?;

    Ok(())
}

pub fn sell(ctx: Context<SellToStaticPoolAccounts>, data: SellToStaticPoolData) -> Result<()> {
    if ctx.accounts.pool.close_if_needed() {
        return err!(StaticPoolError::AlreadyClosed);
    }

    ctx.accounts.pool.collected_lamports = ctx
        .accounts
        .pool
        .collected_lamports
        .checked_sub(data.amount)
        .expect("invariant: lamports amount becomes negative");

    anchor_spl::token::transfer(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::Transfer {
                from: ctx.accounts.user_pool_account.to_account_info(),
                to: ctx.accounts.pool_mint_account.to_account_info(),
                authority: ctx.accounts.user.to_account_info(),
            },
        ),
        data.amount,
    )?;

    ctx.accounts.pool.sub_lamports(data.amount)?;
    ctx.accounts.user.add_lamports(data.amount)?;

    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Default, Clone, PartialEq, PartialOrd)]
pub struct StaticPoolConfig {
    pub min_purchase_lamports: Option<u64>,
    pub close_conditions: PoolCloseConditions,
}

impl StaticPoolConfig {
    pub fn min_purchase_lamports(&self) -> u64 {
        self.min_purchase_lamports.unwrap_or(1)
    }
}

impl Sizable for StaticPoolConfig {
    fn longest() -> Self {
        Self {
            min_purchase_lamports: Some(Sizable::longest()),
            close_conditions: Sizable::longest(),
        }
    }
}

#[account]
#[derive(Default, PartialEq, PartialOrd)]
pub struct StaticPool {
    pub mint: Pubkey,
    pub config: StaticPoolConfig,
    pub state: StaticPoolState,
    pub collected_lamports: u64,
    pub project_id: ProjectId,
    pub bump: u8,
}

impl StaticPool {
    pub fn close_if_needed(&mut self) -> bool {
        if self.config.close_conditions.should_be_closed(
            self.collected_lamports,
            Clock::get().unwrap().unix_timestamp as u64,
        ) {
            self.state = StaticPoolState::Closed;
            true
        } else {
            false
        }
    }
}

impl Sizable for StaticPool {
    fn longest() -> Self {
        Self {
            mint: Default::default(),
            config: Sizable::longest(),
            state: Sizable::longest(),
            collected_lamports: Sizable::longest(),
            project_id: Sizable::longest(),
            bump: Sizable::longest(),
        }
    }
}

#[derive(Clone, Copy, PartialEq, PartialOrd, AnchorSerialize, AnchorDeserialize)]
pub enum StaticPoolState {
    Active,
    Closed,
}

impl Default for StaticPoolState {
    fn default() -> Self {
        Self::Active
    }
}

impl Sizable for StaticPoolState {
    fn longest() -> Self {
        Self::Closed
    }
}

ensure_account_size!(StaticPool, 93);

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateStaticPoolData {
    pub config: StaticPoolConfig,
    pub project_id: ProjectId,
}

#[derive(Accounts)]
#[instruction(data: CreateStaticPoolData)]
pub struct CreateStaticPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        init,
        payer = authority,
        mint::decimals = POOL_TOKEN_DECIMALS,
        mint::authority = authority,
        mint::freeze_authority = authority
    )]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        seeds = [PROJECT_PREFIX, &data.project_id.to_bytes()], bump = project.bump
    )]
    pub project: Account<'info, Project>,

    #[account(
        init,
        payer = authority,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_mint_account: Account<'info, TokenAccount>,

    #[account(
        init,
        payer = authority,
        space = StaticPool::ACCOUNT_SIZE, seeds = [STATIC_POOL_PREFIX, mint.key().as_ref()], bump
    )]
    pub pool: Account<'info, StaticPool>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BuyFromStaticPoolData {
    pub project_id: ProjectId,
    pub amount: u64,
}

#[derive(Accounts)]
#[instruction(data: BuyFromStaticPoolData)]
pub struct BuyFromStaticPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = pool.project_id == project.id,
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
    pub user_mint_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_mint_account: Account<'info, TokenAccount>,

    #[account(mut,
        seeds = [STATIC_POOL_PREFIX, mint.key().as_ref()], bump=pool.bump
    )]
    pub pool: Account<'info, StaticPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
pub struct SellToStaticPoolData {
    pub project_id: ProjectId,
    pub amount: u64,
}

#[derive(Accounts)]
#[instruction(data: SellToStaticPoolData)]
pub struct SellToStaticPoolAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(
        mut,
        constraint = pool.project_id == project.id,
        seeds = [PROJECT_PREFIX, &data.project_id.to_bytes()], bump = project.bump
    )]
    pub project: Account<'info, Project>,

    #[account(mut)]
    pub user: Signer<'info>,

    #[account(mut)]
    pub mint: Account<'info, Mint>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub user_pool_account: Account<'info, TokenAccount>,

    #[account(
        mut,
        associated_token::mint = mint,
        associated_token::authority = pool,
    )]
    pub pool_mint_account: Account<'info, TokenAccount>,

    #[account(mut,
        seeds = [STATIC_POOL_PREFIX, mint.key().as_ref()], bump=pool.bump
    )]
    pub pool: Account<'info, StaticPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[derive(Accounts)]
pub struct GraduateStaticPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    /// CHECK: only for lamports receiving
    #[account(mut)]
    pub funds_receiver: UncheckedAccount<'info>,

    #[account(
        mut,
        seeds = [STATIC_POOL_PREFIX, pool.mint.as_ref()], bump = pool.bump
    )]
    pub pool: Account<'info, StaticPool>,

    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

#[error_code]
pub enum StaticPoolError {
    #[msg("Pool already exists for given project")]
    AlreadyCreated,

    #[msg("Pool is already closed")]
    AlreadyClosed,

    #[msg("Pool is not closed yet")]
    NotClosed,

    #[msg("Pool is not graduated yet")]
    NotGraduated,
}
