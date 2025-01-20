use anchor_lang::{prelude::Pubkey, AnchorDeserialize, AnchorSerialize};
use pumpfun_cpi::BondingCurve;

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

fn constant(curve: &CurveWrapper) -> u128 {
    curve.virtual_sol_reserves as u128 * curve.virtual_token_reserves as u128
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

    pub fn fixed_sols(&self, sols: u64) -> u64 {
        let constant = constant(self.curve);
        let new_token_reserves =
            constant / (self.curve.virtual_sol_reserves as u128 + sols as u128) + 1;
        (self.curve.virtual_token_reserves as u128).saturating_sub(new_token_reserves) as u64
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
        let constant = constant(self.curve);
        let new_token_reserves =
            constant / (self.curve.virtual_sol_reserves as u128 - sols as u128) + 1;
        (new_token_reserves.saturating_sub(self.curve.virtual_token_reserves as u128)) as u64
    }
}
