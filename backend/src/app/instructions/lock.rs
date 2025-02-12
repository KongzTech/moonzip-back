use solana_sdk::pubkey::Pubkey;

const ESCROW_PREFIX: &[u8] = b"escrow";
pub fn escrow_address(base: &Pubkey, program_id: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[ESCROW_PREFIX, base.as_ref()], program_id).0
}
