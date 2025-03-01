use super::{BuyParams, ProjectsOperations, SellParams, WRAPPED_SOL_MINT};
use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account_idempotent,
};
use moonzip::PROGRAM_AUTHORITY;
use once_cell::sync::OnceCell;
use raydium_amm::{
    instruction::swap_base_in,
    processor::{
        get_associated_address_and_bump_seed, AMM_CONFIG_SEED, AUTHORITY_AMM,
        COIN_VAULT_ASSOCIATED_SEED, PC_VAULT_ASSOCIATED_SEED,
    },
};
use solana_sdk::{
    instruction::Instruction,
    program_pack::Pack,
    pubkey::Pubkey,
    system_instruction::{self, create_account_with_seed},
};

static AMM_AUTHORITY: OnceCell<(Pubkey, u8)> = OnceCell::new();
static AMM_CONFIG: OnceCell<(Pubkey, u8)> = OnceCell::new();

impl<'a> ProjectsOperations<'a> {
    pub fn deploy_to_raydium(
        &self,
        tokens_amount: u64,
        sols_amount: u64,
    ) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let donor = PROGRAM_AUTHORITY;
        let market = self.openbook_market_address();

        // Derive AMM pool address
        let amm_pool = self.amm_pool();

        // Create AMM authority
        let (amm_authority, nonce) = self.amm_authority();
        let amm_open_orders = self.amm_open_orders();

        // Derive LP token mint
        let amm_lp_mint = self.amm_lp_mint();

        // Create target orders account
        let amm_target_orders = self.amm_target_orders();

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
        let seed = self.project_derived_seed("raydium_intermediate_holder");

        let user_wrapped_sol_account = Pubkey::create_with_seed(&donor, &seed, &spl_token::ID)?;

        let create_user_wrapped_sol_account = create_account_with_seed(
            &donor,                     // From (payer) account
            &user_wrapped_sol_account,  // To (new) account
            &donor,                     // Base account
            &seed,                      // Seed string
            lamports,                   // Lamports for rent and pool SOL amount
            token_account_space as u64, // Space needed for the account
            &spl_token::ID,             // Owner of the new account
        );
        let initialize_user_wrapped_sol_account = spl_token::instruction::initialize_account(
            &spl_token::ID,
            &user_wrapped_sol_account,
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
            &self.config.raydium_program, // AMM program ID
            &amm_pool,                    // AMM pool account
            &amm_authority,               // AMM authority (PDA)
            &amm_open_orders,             // AMM open orders account
            &amm_lp_mint,                 // LP token mint
            &WRAPPED_SOL_MINT,            // Base/coin token mint (SOL)
            &curve_mint,                  // Quote/pc token mint (Your token)
            &self.amm_coin_vault(),
            &self.amm_pc_vault(),
            &amm_target_orders,                  // Target orders account
            &amm_config,                         // AMM config account
            &create_fee_destination,             // Fee destination account
            &self.config.serum_openbook_program, // OpenBook DEX program
            &market.key,                         // OpenBook market
            &donor,                              // User wallet (payer)
            &user_wrapped_sol_account,           // User's base token account
            &user_token_pc,                      // User's quote token account
            &user_token_lp,                      // User's LP token account
            nonce,                               // PDA nonce of AUTHORITY
            0,                                   // Open time (0 for immediate)
            tokens_amount,                       // Initial PC (quote) amount
            sols_amount,                         // Initial coin (base) amount
        )?;

        let mut ixs = vec![
            create_user_wrapped_sol_account,
            initialize_user_wrapped_sol_account,
            initialize,
            close_user_wrapped_sol_account,
        ];

        let mut burn_and_close_lp = self.burn_and_close(PROGRAM_AUTHORITY, amm_lp_mint)?;
        ixs.append(&mut burn_and_close_lp);

        Ok(ixs)
    }

    pub fn buy_from_raydium(&self, params: BuyParams) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let market = self.openbook_market_address().key;
        let amm_authority = self.amm_authority().0;

        let coin_mint_pk = &WRAPPED_SOL_MINT;
        let market_coin_vault = self.openbook_vault_coin_ata();

        let pc_mint_pk = curve_mint;
        let market_pc_vault = self.openbook_vault_pc_ata();
        let market_vault_signer = self.openbook_vault_pda().0;

        let user_token_source = get_associated_token_address(&params.user, coin_mint_pk);
        let user_token_destination = get_associated_token_address(&params.user, &pc_mint_pk);

        let swap_ix = swap_base_in(
            &self.config.raydium_program,
            &self.amm_pool(),
            &amm_authority,
            &self.amm_open_orders(),
            &self.amm_coin_vault(),
            &self.amm_pc_vault(),
            &self.config.serum_openbook_program,
            &market,
            &self.bids_queue_addr().key,
            &self.asks_queue_addr().key,
            &self.event_queue_addr().key,
            &market_coin_vault,
            &market_pc_vault,
            &market_vault_signer,
            &user_token_source,
            &user_token_destination,
            &params.user,
            params.sols,
            params.min_token_output,
        )?;

        Ok(vec![
            create_associated_token_account_idempotent(
                &params.user,
                &params.user,
                &curve_mint,
                &anchor_spl::token::ID,
            ),
            create_associated_token_account_idempotent(
                &params.user,
                &params.user,
                &WRAPPED_SOL_MINT,
                &anchor_spl::token::ID,
            ),
            system_instruction::transfer(&params.user, &user_token_source, params.sols),
            spl_token::instruction::sync_native(&anchor_spl::token::ID, &user_token_source)?,
            swap_ix,
        ])
    }

    pub fn sell_to_raydium(&self, params: SellParams) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let market = self.openbook_market_address().key;
        let amm_authority = self.amm_authority().0;

        let coin_mint_pk = &WRAPPED_SOL_MINT;
        let market_coin_vault = self.openbook_vault_coin_ata();

        let pc_mint_pk = curve_mint;
        let market_pc_vault = self.openbook_vault_pc_ata();
        let market_vault_signer = self.openbook_vault_pda().0;

        let user_token_source = get_associated_token_address(&params.user, &pc_mint_pk);
        let user_token_destination = get_associated_token_address(&params.user, coin_mint_pk);

        let swap_ix = swap_base_in(
            &self.config.raydium_program,
            &self.amm_pool(),
            &amm_authority,
            &self.amm_open_orders(),
            &self.amm_coin_vault(),
            &self.amm_pc_vault(),
            &self.config.serum_openbook_program,
            &market,
            &self.bids_queue_addr().key,
            &self.asks_queue_addr().key,
            &self.event_queue_addr().key,
            &market_coin_vault,
            &market_pc_vault,
            &market_vault_signer,
            &user_token_source,
            &user_token_destination,
            &params.user,
            params.tokens,
            params.min_sol_output,
        )?;

        Ok(vec![
            create_associated_token_account_idempotent(
                &params.user,
                &params.user,
                &curve_mint,
                &anchor_spl::token::ID,
            ),
            create_associated_token_account_idempotent(
                &params.user,
                &params.user,
                &WRAPPED_SOL_MINT,
                &anchor_spl::token::ID,
            ),
            swap_ix,
            spl_token::instruction::close_account(
                &anchor_spl::token::ID,
                &get_associated_token_address(&params.user, &WRAPPED_SOL_MINT),
                &params.user,
                &params.user,
                &[],
            )?,
        ])
    }

    fn amm_coin_vault(&self) -> Pubkey {
        let (associated_token_address, _) = get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &self.openbook_market_address().key,
            COIN_VAULT_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        associated_token_address
    }

    fn amm_pc_vault(&self) -> Pubkey {
        let (associated_token_address, _) = get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &self.openbook_market_address().key,
            PC_VAULT_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        associated_token_address
    }

    fn amm_open_orders(&self) -> Pubkey {
        let market = self.openbook_market_address().key;

        let (amm_open_orders, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &market,
            raydium_amm::processor::OPEN_ORDER_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        amm_open_orders
    }

    fn amm_pool(&self) -> Pubkey {
        let market = self.openbook_market_address().key;
        let (amm_pool, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &market,
            raydium_amm::processor::AMM_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        amm_pool
    }

    fn amm_lp_mint(&self) -> Pubkey {
        let market = self.openbook_market_address().key;
        let (amm_lp_mint, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &market,
            raydium_amm::processor::LP_MINT_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        amm_lp_mint
    }

    fn amm_target_orders(&self) -> Pubkey {
        let market = self.openbook_market_address().key;
        let (amm_target_orders, _) = raydium_amm::processor::get_associated_address_and_bump_seed(
            &self.config.raydium_program,
            &market,
            raydium_amm::processor::TARGET_ASSOCIATED_SEED,
            &self.config.raydium_program,
        );
        amm_target_orders
    }

    fn amm_authority(&self) -> (Pubkey, u8) {
        *AMM_AUTHORITY.get_or_init(|| {
            Pubkey::find_program_address(&[AUTHORITY_AMM], &self.config.raydium_program)
        })
    }

    fn amm_config(&self) -> (Pubkey, u8) {
        *AMM_CONFIG.get_or_init(|| {
            Pubkey::find_program_address(&[AMM_CONFIG_SEED], &self.config.raydium_program)
        })
    }
}
