use crate::{fee::BasisPoints, utils::Sizable};
use anchor_lang::{AnchorDeserialize, AnchorSerialize};

#[derive(Debug, Clone, Copy, PartialEq, PartialOrd, AnchorSerialize, AnchorDeserialize)]
pub struct CurveConfig {
    pub initial_virtual_sol_reserves: u64,
    pub initial_virtual_token_reserves: u64,
    pub initial_real_token_reserves: u64,
    pub total_token_supply: u64,
}

impl Default for CurveConfig {
    fn default() -> Self {
        Self {
            initial_virtual_token_reserves: 1073000000000000,
            initial_virtual_sol_reserves: 30000000000,
            initial_real_token_reserves: 793100000000000,
            total_token_supply: 1000000000000000,
        }
    }
}

impl Sizable for CurveConfig {
    fn longest() -> Self {
        Self {
            initial_virtual_sol_reserves: Sizable::longest(),
            initial_virtual_token_reserves: Sizable::longest(),
            initial_real_token_reserves: Sizable::longest(),
            total_token_supply: Sizable::longest(),
        }
    }
}

#[derive(
    AnchorDeserialize, AnchorSerialize, Debug, Clone, Copy, PartialEq, PartialOrd, Default,
)]
pub struct CurveState {
    virtual_token_reserves: u64,
    virtual_sol_reserves: u64,
    real_token_reserves: u64,
    real_sol_reserves: u64,
    total_token_supply: u64,
}

impl Sizable for CurveState {
    fn longest() -> Self {
        Self {
            virtual_token_reserves: Sizable::longest(),
            virtual_sol_reserves: Sizable::longest(),
            real_token_reserves: Sizable::longest(),
            real_sol_reserves: Sizable::longest(),
            total_token_supply: Sizable::longest(),
        }
    }
}

impl CurveState {
    pub fn from_cfg(cfg: &CurveConfig) -> Self {
        Self {
            virtual_token_reserves: cfg.initial_virtual_token_reserves,
            virtual_sol_reserves: cfg.initial_virtual_sol_reserves,
            real_token_reserves: cfg.initial_real_token_reserves,
            real_sol_reserves: 0,
            total_token_supply: cfg.total_token_supply,
        }
    }

    pub fn sol_balance(&self) -> u64 {
        self.real_sol_reserves
    }

    pub fn token_balance(&self) -> u64 {
        self.real_token_reserves
    }

    pub fn commit_buy(&mut self, sols: u64, tokens: u64) {
        self.real_token_reserves -= tokens;
        self.virtual_token_reserves -= tokens;

        self.real_sol_reserves += sols;
        self.virtual_sol_reserves += sols;
    }

    pub fn commit_sell(&mut self, tokens: u64, sols: u64) {
        self.real_token_reserves += tokens;
        self.virtual_token_reserves += tokens;

        self.real_sol_reserves -= sols;
        self.virtual_sol_reserves -= sols;
    }

    /// Calculate the product of virtual reserves using u128 to avoid overflow
    fn constant(&self) -> u128 {
        (self.virtual_sol_reserves as u128) * (self.virtual_token_reserves as u128)
    }
}

pub trait CalcBuy {
    /// Shows how much tokens would be received for given fixed amount of sols
    fn fixed_sols(&self, sols: u64) -> u64;
    /// Shows how much sols are needed to buy a given fixed amount of tokens
    fn fixed_tokens(&self, tokens: u64) -> u64;
}

pub struct BuyCalculator<'a> {
    curve: &'a CurveState,
}

impl<'a> BuyCalculator<'a> {
    pub fn new(curve: &'a CurveState) -> Self {
        Self { curve }
    }
}
impl<'a> CalcBuy for BuyCalculator<'a> {
    fn fixed_sols(&self, sols: u64) -> u64 {
        let constant = self.curve.constant();

        // Calculate the new virtual sol reserves after the purchase
        let new_sol_reserves: u128 = (self.curve.virtual_sol_reserves as u128) + (sols as u128);

        // Calculate the new virtual token reserves after the purchase
        let new_token_reserves: u128 = constant / new_sol_reserves + 1;

        // Calculate the amount of tokens to be purchased
        let result: u128 =
            (self.curve.virtual_token_reserves as u128).saturating_sub(new_token_reserves);

        result as u64
    }

    fn fixed_tokens(&self, tokens: u64) -> u64 {
        let constant = self.curve.constant();
        let new_tokens_reserves = self.curve.virtual_token_reserves as u128 + tokens as u128;
        let new_sol_reserves = constant / new_tokens_reserves + 1;
        self.curve
            .virtual_sol_reserves
            .saturating_sub(new_sol_reserves as u64)
    }
}

pub struct BuyCalculatorWithFee<'a> {
    calculator: BuyCalculator<'a>,
    fee: BasisPoints,
}

impl<'a> BuyCalculatorWithFee<'a> {
    pub fn new(calculator: BuyCalculator<'a>, fee: BasisPoints) -> Self {
        Self { calculator, fee }
    }
}

impl<'a> CalcBuy for BuyCalculatorWithFee<'a> {
    fn fixed_sols(&self, sols: u64) -> u64 {
        let fee = self.fee.part_of(sols);
        let resulting_sol = sols.saturating_sub(fee);
        self.calculator.fixed_sols(resulting_sol)
    }

    fn fixed_tokens(&self, tokens: u64) -> u64 {
        let result = self.calculator.fixed_tokens(tokens);
        let applied_fee = self.fee.on_top_of(result);
        result.saturating_add(applied_fee)
    }
}

pub trait CalcSell {
    /// Shows how much sols would be received for a fixed amount of tokens
    fn fixed_tokens(&self, tokens: u64) -> u64;

    /// Shows how much tokens need to be sold to get a fixed amount of SOL
    fn fixed_sols(&self, sols: u64) -> u64;
}

pub struct SellCalculator<'a> {
    curve: &'a CurveState,
}

impl<'a> SellCalculator<'a> {
    pub fn new(curve: &'a CurveState) -> Self {
        Self { curve }
    }

    pub fn with_fee(self, fee: BasisPoints) -> impl CalcSell + 'a {
        SellCalculatorWithFee::new(self, fee)
    }
}

impl<'a> CalcSell for SellCalculator<'a> {
    fn fixed_tokens(&self, tokens: u64) -> u64 {
        let constant = self.curve.constant();
        let new_token_reserves = self.curve.virtual_token_reserves as u128 + tokens as u128;
        let new_sol_reserves = constant / new_token_reserves + 1;
        self.curve
            .virtual_sol_reserves
            .saturating_sub(new_sol_reserves as u64)
    }

    fn fixed_sols(&self, sols: u64) -> u64 {
        let constant = self.curve.constant();
        let new_token_reserves =
            constant / (self.curve.virtual_sol_reserves as u128 - sols as u128) + 1;
        (new_token_reserves.saturating_sub(self.curve.virtual_token_reserves as u128)) as u64
    }
}

pub struct SellCalculatorWithFee<'a> {
    calculator: SellCalculator<'a>,
    fee: BasisPoints,
}

impl<'a> SellCalculatorWithFee<'a> {
    pub fn new(calculator: SellCalculator<'a>, fee: BasisPoints) -> Self {
        Self { calculator, fee }
    }
}

impl<'a> CalcSell for SellCalculatorWithFee<'a> {
    fn fixed_tokens(&self, tokens: u64) -> u64 {
        let result = self.calculator.fixed_tokens(tokens);
        let fee = self.fee.part_of(result);
        result.saturating_sub(fee)
    }

    fn fixed_sols(&self, sols: u64) -> u64 {
        let fee = self.fee.part_of(sols);
        let resulting_sols = sols.saturating_sub(fee);
        self.calculator.fixed_sols(resulting_sols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    impl CurveState {
        pub fn intial_pumpfun() -> Self {
            Self {
                virtual_token_reserves: 1073000000000000,
                virtual_sol_reserves: 30000000000,
                real_token_reserves: 793100000000000,
                real_sol_reserves: 0,
                total_token_supply: 1000000000000000,
            }
        }
    }
}
