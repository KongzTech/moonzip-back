use mpl_token_metadata::types::Key;
use once_cell::sync::Lazy;
use solana_sdk::{native_token::sol_to_lamports, pubkey::Pubkey, rent::Rent};
use std::str::FromStr;

const METADATA_PREFIX: &[u8] = b"metadata";
pub static MPL_FEE: Lazy<u64> = Lazy::new(|| sol_to_lamports(0.01));
pub const SAMPLE_MPL_URI: &str =
    "https://ipfs.io/ipfs/QmY7kbXawfNxrBRCyGshhNbsdLryHrqgHjv3DgJVc64d2g";
pub static PROGRAM: Lazy<Pubkey> =
    Lazy::new(|| Pubkey::from_str("metaqbxxUerdq28cj1RbAWkYQm3ybzjb6a8bt518x1s").unwrap());

pub fn metadata_account(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[METADATA_PREFIX, PROGRAM.as_ref(), mint.as_ref()],
        &PROGRAM,
    )
    .0
}

#[derive(Debug, Clone, Copy)]
pub struct SampleMetadata<'a> {
    pub name: &'a str,
    pub symbol: &'a str,
    pub uri: &'a str,
}

impl<'a> SampleMetadata<'a> {
    pub fn estimate_price(&self, rent: &Rent) -> anyhow::Result<u64> {
        let size = borsh::to_vec(&self.as_mpl())?.len();
        let price = rent.minimum_balance(size) + *MPL_FEE;
        Ok(price)
    }

    fn as_mpl(&self) -> mpl_token_metadata::accounts::Metadata {
        mpl_token_metadata::accounts::Metadata {
            key: Key::MetadataV1,
            update_authority: Pubkey::default(),
            mint: Pubkey::default(),
            name: self.name.to_string(),
            symbol: self.symbol.to_string(),
            uri: self.uri.to_string(),
            seller_fee_basis_points: 0,
            creators: None,
            primary_sale_happened: false,
            is_mutable: false,
            edition_nonce: None,
            token_standard: Some(mpl_token_metadata::types::TokenStandard::Fungible),
            collection: None,
            uses: None,
            collection_details: None,
            programmable_config: None,
        }
    }
}
