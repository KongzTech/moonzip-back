use once_cell::sync::Lazy;
use solana_sdk::pubkey::Pubkey;
use std::str::FromStr;

const METADATA_PREFIX: &[u8] = b"metadata";
pub static PROGRAM: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap());

pub fn metadata_account(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[METADATA_PREFIX, PROGRAM.as_ref(), mint.as_ref()],
        &PROGRAM,
    )
    .0
}
