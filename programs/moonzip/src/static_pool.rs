use crate::{common::PoolCloseConditions, ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::{prelude::*, system_program};
use anchor_spl::{
    associated_token::AssociatedToken,
    token::{Mint, MintTo, Token, TokenAccount},
};

// matches native SOL decimals so to be 1:1 with it.
const POOL_TOKEN_DECIMALS: u8 = 9;
pub const STATIC_POOL_PREFIX: &[u8] = b"static-pool";

pub fn create(ctx: Context<CreateStaticPoolAccounts>, data: CreateStaticPoolData) -> Result<()> {
    system_program::create_account(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::CreateAccount {
                from: ctx.accounts.authority.to_account_info(),
                to: ctx.accounts.mint.to_account_info(),
            },
        ),
        Rent::get()?.minimum_balance(Mint::LEN),
        Mint::LEN as u64,
        ctx.accounts.token_program.key,
    )?;
    anchor_spl::token::initialize_mint2(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            anchor_spl::token::InitializeMint2 {
                mint: ctx.accounts.mint.to_account_info(),
            },
        ),
        POOL_TOKEN_DECIMALS,
        ctx.accounts.authority.key,
        None,
    )?;

    ctx.accounts.pool.set_inner(StaticPool {
        mint: *ctx.accounts.mint.key,
        config: data.config,
        collected_lamports: 0,
        state: StaticPoolState::Active,
        bump: ctx.bumps.pool,
    });

    Ok(())
}

pub fn graduate(ctx: Context<GraduateStaticPoolAccounts>) -> Result<()> {
    if !matches!(ctx.accounts.pool.state, StaticPoolState::Closed) {
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

    ctx.accounts.pool.close_if_needed();

    anchor_spl::token::mint_to(
        CpiContext::new(
            ctx.accounts.token_program.to_account_info(),
            MintTo {
                mint: ctx.accounts.mint.to_account_info(),
                to: ctx.accounts.user_pool_account.to_account_info(),
                authority: ctx.accounts.authority.to_account_info(),
            },
        ),
        amount,
    )?;
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

#[derive(AnchorSerialize, AnchorDeserialize, Default, Clone, PartialEq, PartialOrd)]
pub struct StaticPoolConfig {
    min_purchase_lamports: Option<u64>,
    close_conditions: PoolCloseConditions,
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
    mint: Pubkey,
    config: StaticPoolConfig,
    state: StaticPoolState,
    collected_lamports: u64,
    bump: u8,
}

impl StaticPool {
    pub fn close_if_needed(&mut self) {
        if self
            .config
            .close_conditions
            .should_be_closed(self.collected_lamports)
        {
            self.state = StaticPoolState::Closed;
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

ensure_account_size!(StaticPool, 77);

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct CreateStaticPoolData {
    pub config: StaticPoolConfig,
}

#[derive(Accounts)]
#[instruction(data: CreateStaticPoolData)]
pub struct CreateStaticPoolAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub mint: Signer<'info>,

    #[account(
        init,
        payer = authority,
        space = StaticPool::ACCOUNT_SIZE, seeds = [STATIC_POOL_PREFIX, mint.key().as_ref()], bump
    )]
    pub pool: Account<'info, StaticPool>,
    pub system_program: Program<'info, System>,
    pub token_program: Program<'info, Token>,
}

#[derive(AnchorSerialize, AnchorDeserialize)]
pub struct BuyFromStaticPoolData {
    pub amount: u64,
}

#[derive(Accounts)]
pub struct BuyFromStaticPoolAccounts<'info> {
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
    pub user_pool_account: Account<'info, TokenAccount>,

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
    #[msg("Pool is not closed yet")]
    NotClosed,

    #[msg("Pool is not graduated yet")]
    NotGraduated,
}
