use solana_program::{pubkey::Pubkey, system_instruction::create_account_with_seed};
use solana_sdk::{instruction::Instruction, rent::Rent};

const MARKET_SEED: &str = "mzip_market";

pub struct OpenbookInstructionsBuilder<'a> {
    pub rent: &'a Rent,
    pub payer: &'a Pubkey,
    pub mint: &'a Pubkey,
    pub program_id: &'a Pubkey,
}

impl<'a> OpenbookInstructionsBuilder<'a> {
    pub fn create_market_account(&self) -> anyhow::Result<(Pubkey, Instruction)> {
        let space = 388;
        let lamports = self.rent.minimum_balance(space);
        let market_address = self.market_address();

        // Create the instruction
        let ix = create_account_with_seed(
            self.payer,      // From (payer) account
            &market_address, // To (new) account
            self.mint,       // Base account
            MARKET_SEED,     // Seed string
            lamports,        // Lamports for rent
            space as u64,    // Space needed for the account
            self.program_id, // Owner of the new account
        );

        Ok((market_address, ix))
    }

    // Similar functions for other accounts
    pub fn create_queue_account(
        &self,
        seed_suffix: &str, // "req_q" or "event_q"
        space: usize,
    ) -> anyhow::Result<(Pubkey, Instruction)> {
        let seed = format!("queue_{}", seed_suffix);

        let queue_address = Pubkey::create_with_seed(self.mint, &seed, self.program_id)?;
        let lamports = self.rent.minimum_balance(space);

        let ix = create_account_with_seed(
            self.payer,
            &queue_address,
            self.mint,
            &seed,
            lamports,
            space as u64,
            self.program_id,
        );

        Ok((queue_address, ix))
    }

    pub fn market_address(&self) -> Pubkey {
        Pubkey::create_with_seed(self.mint, MARKET_SEED, self.program_id).unwrap()
    }
}
