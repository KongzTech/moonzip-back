use super::{ProjectsOperations, WRAPPED_SOL_MINT};
use anchor_spl::associated_token::get_associated_token_address;
use moonzip::PROGRAM_AUTHORITY;
use once_cell::sync::OnceCell;
use raydium_amm::processor::{AMM_ASSOCIATED_SEED, AMM_CONFIG_SEED};
use solana_sdk::{
    instruction::Instruction, program_pack::Pack, pubkey::Pubkey,
    system_instruction::create_account_with_seed,
};

static AMM_AUTHORITY: OnceCell<(Pubkey, u8)> = OnceCell::new();
static AMM_CONFIG: OnceCell<(Pubkey, u8)> = OnceCell::new();

impl<'a> ProjectsOperations<'a> {
    pub fn deploy_to_raydium(
        &self,
        openbook_market: &Pubkey,
        tokens_amount: u64,
        sols_amount: u64,
    ) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let donor = PROGRAM_AUTHORITY;

        // Derive AMM pool address
        let (amm_pool, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &raydium_amm::ID,
            openbook_market,
            raydium_amm::processor::AMM_ASSOCIATED_SEED,
            &raydium_amm::ID,
        );

        // Create AMM authority
        let (amm_authority, nonce) = self.amm_authority();

        // Create AMM open orders account
        let (amm_open_orders, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &raydium_amm::ID,
            &amm_pool,
            raydium_amm::processor::OPEN_ORDER_ASSOCIATED_SEED,
            &raydium_amm::ID,
        );

        // Derive LP token mint
        let (amm_lp_mint, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &raydium_amm::ID,
            &amm_pool,
            raydium_amm::processor::LP_MINT_ASSOCIATED_SEED,
            &raydium_amm::ID,
        );

        // Create target orders account
        let (amm_target_orders, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &raydium_amm::ID,
            &amm_pool,
            raydium_amm::processor::TARGET_ASSOCIATED_SEED,
            &raydium_amm::ID,
        );

        // Get token vaults
        let amm_coin_vault = get_associated_token_address(&amm_authority, &WRAPPED_SOL_MINT);
        let amm_pc_vault = get_associated_token_address(&amm_authority, &curve_mint);

        // Get user token accounts
        let user_token_pc = get_associated_token_address(&donor, &curve_mint);
        let user_token_lp = get_associated_token_address(&donor, &amm_lp_mint);

        // Get AMM config account
        let (amm_config, _) = self.amm_config();

        // Create fee destination account
        let create_fee_destination =
            raydium_amm::processor::config_feature::create_pool_fee_address::id();

        let token_account_space = spl_token::state::Account::LEN;
        let lamports = self.rent.minimum_balance(token_account_space) + sols_amount;

        let user_wrapped_sol_account =
            Pubkey::create_with_seed(&donor, &curve_mint.to_string(), &spl_token::ID)?;

        let create_user_wrapped_sol_account = create_account_with_seed(
            &donor,                     // From (payer) account
            &user_wrapped_sol_account,  // To (new) account
            &donor,                     // Base account
            &curve_mint.to_string(),    // Seed string
            lamports,                   // Lamports for rent and pool SOL amount
            token_account_space as u64, // Space needed for the account
            &spl_token::ID,             // Owner of the new account
        );
        let initialize_user_wrapped_sol_account = spl_token::instruction::initialize_account(
            &spl_token::ID,
            &donor,
            &WRAPPED_SOL_MINT,
            &donor,
        )?;
        let close_user_wrapped_sol_account = spl_token::instruction::close_account(
            &spl_token::ID,
            &user_wrapped_sol_account,
            &donor,
            &donor,
            &[&donor],
        )?;

        let initialize = raydium_amm::instruction::initialize2(
            &raydium_amm::ID,                    // AMM program ID
            &amm_pool,                           // AMM pool account
            &amm_authority,                      // AMM authority (PDA)
            &amm_open_orders,                    // AMM open orders account
            &amm_lp_mint,                        // LP token mint
            &WRAPPED_SOL_MINT,                   // Base/coin token mint (SOL)
            &curve_mint,                         // Quote/pc token mint (Your token)
            &amm_coin_vault,                     // Base/coin token vault
            &amm_pc_vault,                       // Quote/pc token vault
            &amm_target_orders,                  // Target orders account
            &amm_config,                         // AMM config account
            &create_fee_destination,             // Fee destination account
            &self.config.serum_openbook_program, // OpenBook DEX program
            openbook_market,                     // OpenBook market
            &PROGRAM_AUTHORITY,                  // User wallet (payer)
            &user_wrapped_sol_account,           // User's base token account
            &user_token_pc,                      // User's quote token account
            &user_token_lp,                      // User's LP token account
            nonce,                               // PDA nonce of AUTHORITY
            0,                                   // Open time (0 for immediate)
            tokens_amount,                       // Initial PC (quote) amount
            sols_amount,                         // Initial coin (base) amount
        )?;
        Ok(vec![
            create_user_wrapped_sol_account,
            initialize_user_wrapped_sol_account,
            initialize,
            close_user_wrapped_sol_account,
        ])
    }

    fn amm_authority(&self) -> (Pubkey, u8) {
        *AMM_AUTHORITY
            .get_or_init(|| Pubkey::find_program_address(&[AMM_ASSOCIATED_SEED], &raydium_amm::ID))
    }

    fn amm_config(&self) -> (Pubkey, u8) {
        *AMM_CONFIG
            .get_or_init(|| Pubkey::find_program_address(&[AMM_CONFIG_SEED], &raydium_amm::ID))
    }
}
