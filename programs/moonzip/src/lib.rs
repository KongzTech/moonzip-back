use anchor_lang::prelude::*;
use const_str_to_pubkey::str_to_pubkey;
pub mod common;
pub mod curved_pool;
pub mod pumpfun;
pub mod static_pool;
pub mod transmuter;
pub mod utils;

declare_id!("544hmhQ5N72wv8aJFz92sgRMnDEqwmSuzGtG8T8CPgNb");
const PROGRAM_AUTHORITY: Pubkey = str_to_pubkey(env!("MOONZIP_AUTHORITY"));

#[program]
pub mod moonzip {
    pub use super::curved_pool::global::*;
    pub use super::curved_pool::*;
    pub use super::static_pool::*;
    pub use super::transmuter::*;
    use super::*;

    pub fn create_static_pool(
        ctx: Context<CreateStaticPoolAccounts>,
        data: CreateStaticPoolData,
    ) -> Result<()> {
        static_pool::create(ctx, data)
    }

    pub fn graduate_static_pool(ctx: Context<GraduateStaticPoolAccounts>) -> Result<()> {
        static_pool::graduate(ctx)
    }

    pub fn buy_from_static_pool(
        ctx: Context<BuyFromStaticPoolAccounts>,
        data: BuyFromStaticPoolData,
    ) -> Result<()> {
        static_pool::buy(ctx, data)
    }

    pub fn set_curved_pool_global_config(
        ctx: Context<SetCurvedPoolGlobalConfigAccounts>,
        config: GlobalCurvedPoolConfig,
    ) -> Result<()> {
        curved_pool::global::set_global_config(ctx, config)
    }

    pub fn create_curved_pool(
        ctx: Context<CreateCurvedPoolAccounts>,
        data: CreateCurvedPoolData,
    ) -> Result<()> {
        curved_pool::create(ctx, data)
    }

    pub fn graduate_curved_pool(ctx: Context<GraduateCurvedPoolAccounts>) -> Result<()> {
        curved_pool::graduate(ctx)
    }

    pub fn buy_from_curved_pool(
        ctx: Context<BuyFromCurvedPoolAccounts>,
        data: BuyFromCurvedPoolData,
    ) -> Result<()> {
        curved_pool::buy(ctx, data)
    }

    pub fn sell_from_curved_pool(
        ctx: Context<SellFromCurvedPoolAccounts>,
        data: SellFromCurvedPoolData,
    ) -> Result<()> {
        curved_pool::sell(ctx, data)
    }

    pub fn create_transmuter(ctx: Context<CreateTransmuterAccounts>) -> Result<()> {
        transmuter::create(ctx)
    }

    pub fn init_transmuter_for_curve(ctx: Context<InitTransmuterForCurveAccounts>) -> Result<()> {
        transmuter::init_for_curve(ctx)
    }

    pub fn init_transmuter_for_pumpfun_curve(
        ctx: Context<InitTransmuterForPumpfunCurveAccounts>,
    ) -> Result<()> {
        transmuter::init_for_pumpfun_curve(ctx)
    }

    pub fn transmute(ctx: Context<TransmuteAccounts>, data: TransmuteData) -> Result<()> {
        transmuter::transmute(ctx, data)
    }
}
