use anchor_lang::{
    prelude::*, solana_program::instruction::Instruction, AnchorDeserialize, AnchorSerialize,
    InstructionData,
};
use anchor_spl::{associated_token::AssociatedToken, token::TokenAccount};
use pumpfun_cpi::BondingCurve;
use seeds::BONDING_CURVE_SEED;

use crate::BasisPoints;

pub fn buy(ctx: Context<BuyFromPumpAccounts>, data: BuyFromPumpData) -> Result<()> {
    let calculator = BuyCalculator::from_cpi_curve(&ctx.accounts.bonding_curve);
    let buy_params = calculator.fixed_sols(data.sols);
    if buy_params.tokens < data.min_token_output {
        return err!(PumpfunError::SlippageViolated);
    }
    let buy_params = buy_params.as_ix_data();
    let accounts = pumpfun_cpi::accounts::Buy {
        global: ctx.accounts.global.key(),
        fee_recipient: ctx.accounts.fee_recipient.key(),
        mint: ctx.accounts.mint.key(),
        bonding_curve: ctx.accounts.bonding_curve.key(),
        associated_bonding_curve: ctx.accounts.associated_bonding_curve.key(),
        associated_user: ctx.accounts.associated_user.key(),
        user: ctx.accounts.user.key(),
        token_program: ctx.accounts.token_program.key(),
        system_program: ctx.accounts.system_program.key(),
        rent: ctx.accounts.rent.key(),
        event_authority: ctx.accounts.event_authority.key(),
        program: ctx.accounts.program.key(),
    };

    let instruction = Instruction {
        program_id: ctx.accounts.program.key(),
        data: buy_params.data(),
        accounts: accounts.to_account_metas(None),
    };
    anchor_lang::solana_program::program::invoke(&instruction, &ctx.accounts.to_account_infos())?;

    Ok(())
}

#[derive(AnchorSerialize, AnchorDeserialize, Default, Debug)]
pub struct BuyFromPumpData {
    pub sols: u64,
    pub min_token_output: u64,
}

#[derive(Accounts)]
pub struct BuyFromPumpAccounts<'info> {
    /// CHECK: we don't check accounts, as they are further sent to pumpfun via CPI
    pub global: UncheckedAccount<'info>,

    /// CHECK: same
    pub event_authority: UncheckedAccount<'info>,

    /// CHECK: same
    #[account(mut)]
    pub fee_recipient: UncheckedAccount<'info>,

    /// CHECK: same
    pub mint: UncheckedAccount<'info>,
    /// CHECK: same
    #[account(
        mut,
        seeds = [BONDING_CURVE_SEED, mint.key().as_ref()], bump,
        seeds::program = pumpfun_cpi::ID
    )]
    pub bonding_curve: Account<'info, BondingCurve>,
    /// CHECK: same
    #[account(mut)]
    pub associated_bonding_curve: UncheckedAccount<'info>,

    /// CHECK: same
    #[account(
        init_if_needed,
        payer = user,
        associated_token::mint = mint,
        associated_token::authority = user,
    )]
    pub associated_user: Account<'info, TokenAccount>,
    #[account(mut)]
    pub user: Signer<'info>,

    /// CHECK: same
    pub system_program: UncheckedAccount<'info>,
    /// CHECK: same
    pub token_program: UncheckedAccount<'info>,
    /// CHECK: same
    pub rent: UncheckedAccount<'info>,
    /// CHECK: same
    pub program: UncheckedAccount<'info>,
    pub associated_token_program: Program<'info, AssociatedToken>,
}

pub const SELL_FEE: BasisPoints = BasisPoints(100);
pub const BUY_FEE: BasisPoints = BasisPoints(100);

pub mod seeds {
    /// Seed for the global state PDA
    pub const GLOBAL_SEED: &[u8] = b"global";

    /// Seed for the mint authority PDA
    pub const MINT_AUTHORITY_SEED: &[u8] = b"mint-authority";

    /// Seed for bonding curve PDAs
    pub const BONDING_CURVE_SEED: &[u8] = b"bonding-curve";
}

pub fn get_bonding_curve_pda(mint: &Pubkey) -> Option<Pubkey> {
    let seeds: &[&[u8]; 2] = &[seeds::BONDING_CURVE_SEED, mint.as_ref()];
    let program_id: &Pubkey = &pumpfun_cpi::ID;
    let pda: Option<(Pubkey, u8)> = Pubkey::try_find_program_address(seeds, program_id);
    pda.map(|pubkey| pubkey.0)
}

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone, Default)]
pub struct CurveWrapper {
    pub virtual_token_reserves: u64,
    pub virtual_sol_reserves: u64,
    pub real_token_reserves: u64,
    pub real_sol_reserves: u64,
    pub token_total_supply: u64,
}

impl CurveWrapper {
    pub fn initial(global: &pumpfun_cpi::Global) -> Self {
        Self {
            virtual_token_reserves: global.initial_virtual_token_reserves,
            virtual_sol_reserves: global.initial_virtual_sol_reserves,
            real_token_reserves: global.initial_real_token_reserves,
            real_sol_reserves: 0,
            token_total_supply: global.token_total_supply,
        }
    }

    pub fn commit_buy(&mut self, sols: u64, tokens: u64) {
        self.real_token_reserves -= tokens;
        self.virtual_token_reserves -= tokens;

        self.real_sol_reserves += sols;
        self.virtual_sol_reserves += sols;
    }

    fn constant(&self) -> u128 {
        self.virtual_sol_reserves as u128 * self.virtual_token_reserves as u128
    }
}

impl From<BondingCurve> for CurveWrapper {
    fn from(curve: BondingCurve) -> Self {
        Self {
            virtual_token_reserves: curve.virtual_token_reserves,
            virtual_sol_reserves: curve.virtual_sol_reserves,
            real_token_reserves: curve.real_token_reserves,
            real_sol_reserves: curve.real_sol_reserves,
            token_total_supply: curve.token_total_supply,
        }
    }
}

pub struct BuyCalculator {
    virtual_token_reserves: u64,
    virtual_sol_reserves: u64,
}

impl BuyCalculator {
    pub fn from_curve_wrapper(curve: &CurveWrapper) -> Self {
        Self {
            virtual_token_reserves: curve.virtual_token_reserves,
            virtual_sol_reserves: curve.virtual_sol_reserves,
        }
    }

    pub fn from_cpi_curve(curve: &BondingCurve) -> Self {
        Self {
            virtual_token_reserves: curve.virtual_token_reserves,
            virtual_sol_reserves: curve.virtual_sol_reserves,
        }
    }

    pub fn fixed_sols(&self, sols: u64) -> BuyParams {
        let after_fee_taken = BUY_FEE.accounting(sols);

        let constant = self.constant();
        let new_token_reserves =
            constant / (self.virtual_sol_reserves as u128 + after_fee_taken as u128) + 1;
        let tokens =
            (self.virtual_token_reserves as u128).saturating_sub(new_token_reserves) as u64;
        BuyParams {
            tokens,
            // we place original sols amount there, because pumpfun includes *fee* into slippage.
            // in other words, if total amount of sols greater then `max_sol_cost`, it would throw
            // error.
            max_sol_cost: sols,
        }
    }

    fn constant(&self) -> u128 {
        self.virtual_token_reserves as u128 * self.virtual_sol_reserves as u128
    }
}

#[derive(Debug, Clone, Copy)]
pub struct BuyParams {
    pub tokens: u64,
    pub max_sol_cost: u64,
}

impl BuyParams {
    pub fn as_ix_data(&self) -> pumpfun_cpi::instruction::Buy {
        pumpfun_cpi::instruction::Buy {
            _amount: self.tokens,
            _max_sol_cost: self.max_sol_cost,
        }
    }
}

pub struct SellCalculator<'a> {
    curve: &'a CurveWrapper,
}

impl<'a> SellCalculator<'a> {
    pub fn new(curve: &'a CurveWrapper) -> Self {
        Self { curve }
    }

    /// Shows how much tokens need to be sold to get a fixed amount of SOL
    pub fn fixed_sols(&self, sols: u64) -> u64 {
        let constant = self.curve.constant();
        let new_token_reserves =
            constant / (self.curve.virtual_sol_reserves as u128 - sols as u128) + 1;
        (new_token_reserves.saturating_sub(self.curve.virtual_token_reserves as u128)) as u64
    }
}

#[error_code]
pub enum PumpfunError {
    #[msg("Set slippage setting is violated: price changed")]
    SlippageViolated,
}
