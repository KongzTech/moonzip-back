use anchor_spl::associated_token::{
    get_associated_token_address,
    spl_associated_token_account::instruction::create_associated_token_account,
};
use moonzip::PROGRAM_AUTHORITY;
use serum_dex::instruction::initialize_market;
use solana_program::{pubkey::Pubkey, system_instruction::create_account_with_seed};
use solana_sdk::instruction::Instruction;

use crate::utils::find_program_address_with_u64_nonce;

use super::{ProjectsOperations, SeedDerivedPubkey, WRAPPED_SOL_MINT};

impl<'a> ProjectsOperations<'a> {
    pub fn prepare_openbook_market_vaults(&self) -> anyhow::Result<Vec<Instruction>> {
        let vault = self.openbook_vault_pda().0;
        Ok(vec![
            create_associated_token_account(
                &PROGRAM_AUTHORITY,
                &vault,
                &WRAPPED_SOL_MINT,
                &anchor_spl::token::ID,
            ),
            create_associated_token_account(
                &PROGRAM_AUTHORITY,
                &vault,
                &self.curve_mint()?,
                &anchor_spl::token::ID,
            ),
        ])
    }

    pub fn initialize_openbook_market(&self) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let market = self.openbook_market_address();
        let create_market_ix = self.create_market_account()?;
        let (_, vault_nonce) = self.openbook_vault_pda();

        let bids = self.bids_queue_addr();
        let asks = self.asks_queue_addr();
        let requests = self.request_queue_addr();
        let events = self.event_queue_addr();

        let request_queue_ix = self.create_queue_account(&requests, 764)?;
        let event_queue_ix = self.create_queue_account(&events, 11308)?;
        let bids_ix = self.create_queue_account(&bids, 14524)?;
        let asks_ix = self.create_queue_account(&asks, 14524)?;

        let coin_mint_pk = &WRAPPED_SOL_MINT;
        let coin_vault_pk = self.openbook_vault_coin_ata();

        let pc_mint_pk = curve_mint;
        let pc_vault_pk = self.openbook_vault_pc_ata();

        let initialize_market_ix = initialize_market(
            &market.key,
            &self.config.serum_openbook_program,
            coin_mint_pk,
            &pc_mint_pk,
            &coin_vault_pk,
            &pc_vault_pk,
            None,
            None,
            None,
            &bids.key,
            &asks.key,
            &requests.key,
            &events.key,
            10000000,
            100,
            vault_nonce,
            100,
        )?;

        Ok(vec![
            create_market_ix,
            bids_ix,
            asks_ix,
            request_queue_ix,
            event_queue_ix,
            initialize_market_ix,
        ])
    }

    pub fn openbook_vault_coin_ata(&self) -> Pubkey {
        get_associated_token_address(&self.openbook_vault_pda().0, &WRAPPED_SOL_MINT)
    }

    pub fn openbook_vault_pc_ata(&self) -> Pubkey {
        get_associated_token_address(&self.openbook_vault_pda().0, &self.curve_mint().unwrap())
    }

    pub fn create_market_account(&self) -> anyhow::Result<Instruction> {
        let space = 388;
        let lamports = self.rent.minimum_balance(space);
        let key = self.openbook_market_address();
        let payer = PROGRAM_AUTHORITY;

        // Create the instruction
        let ix = create_account_with_seed(
            &payer,
            &key.key,
            &payer,
            &key.seed,
            lamports,
            space as u64,
            &self.config.serum_openbook_program,
        );

        Ok(ix)
    }

    pub fn create_queue_account(
        &self,
        key: &SeedDerivedPubkey,
        space: usize,
    ) -> anyhow::Result<Instruction> {
        let payer = PROGRAM_AUTHORITY;
        let lamports = self.rent.minimum_balance(space);

        let ix = create_account_with_seed(
            &payer,
            &key.key,
            &payer,
            &key.seed,
            lamports,
            space as u64,
            &self.config.serum_openbook_program,
        );

        Ok(ix)
    }

    pub fn bids_queue_addr(&self) -> SeedDerivedPubkey {
        self.derive_queue_address("bids")
    }

    pub fn asks_queue_addr(&self) -> SeedDerivedPubkey {
        self.derive_queue_address("asks")
    }

    pub fn event_queue_addr(&self) -> SeedDerivedPubkey {
        self.derive_queue_address("event")
    }

    pub fn request_queue_addr(&self) -> SeedDerivedPubkey {
        self.derive_queue_address("request")
    }

    fn derive_queue_address(&self, seed_suffix: &str) -> SeedDerivedPubkey {
        let seed = self.project_derived_seed(&format!("openbook_queue_{}", seed_suffix));
        let queue_address = Pubkey::create_with_seed(
            &PROGRAM_AUTHORITY,
            &seed,
            &self.config.serum_openbook_program,
        )
        .unwrap();
        SeedDerivedPubkey {
            key: queue_address,
            seed,
        }
    }

    pub fn openbook_market_address(&self) -> SeedDerivedPubkey {
        let seed = self.project_derived_seed("openbook_market");
        SeedDerivedPubkey {
            key: Pubkey::create_with_seed(
                &PROGRAM_AUTHORITY,
                &seed,
                &self.config.serum_openbook_program,
            )
            .unwrap(),
            seed,
        }
    }

    pub fn openbook_vault_pda(&self) -> (Pubkey, u64) {
        find_program_address_with_u64_nonce(
            &[self.openbook_market_address().key.as_ref()],
            &self.config.serum_openbook_program,
        )
        .expect("unable to find openbook vault PDA")
    }
}
