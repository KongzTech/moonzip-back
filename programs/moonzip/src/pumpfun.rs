use anchor_lang::{prelude::Pubkey, AnchorDeserialize, AnchorSerialize};
use pumpfun_cpi::BondingCurve;

use crate::BasisPoints;

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

#[derive(AnchorSerialize, AnchorDeserialize, Debug, Clone)]
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

pub struct BuyCalculator<'a> {
    curve: &'a CurveWrapper,
}

impl<'a> BuyCalculator<'a> {
    pub fn new(curve: &'a CurveWrapper) -> Self {
        Self { curve }
    }

    pub fn fixed_sols(&self, sols: u64) -> BuyParams {
        let after_fee_taken = BUY_FEE.accounting(sols);

        let constant = self.curve.constant();
        let new_token_reserves =
            constant / (self.curve.virtual_sol_reserves as u128 + after_fee_taken as u128) + 1;
        let tokens =
            (self.curve.virtual_token_reserves as u128).saturating_sub(new_token_reserves) as u64;
        BuyParams {
            tokens,
            // we place original sols amount there, because pumpfun includes *fee* into slippage.
            // in other words, if total amount of sols greater then `max_sol_cost`, it would throw
            // error.
            max_sol_cost: sols,
        }
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
