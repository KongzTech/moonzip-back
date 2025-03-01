use bytemuck::bytes_of;
use solana_sdk::pubkey::{Pubkey, PubkeyError};

/// It is extracted from anchor event codegen.
/// Check there: https://docs.rs/anchor-lang/latest/anchor_lang/attr.event.html
pub fn anchor_event_discriminator(event_struct_name: &str) -> [u8; 8] {
    let discriminator_preimage = format!("event:{event_struct_name}").into_bytes();
    let discriminator = anchor_syn::hash::hash(&discriminator_preimage);
    discriminator.0[..8].try_into().unwrap()
}

/// Helps defining static discriminators for generated events.
/// It isn't correctly done by anchor.
#[macro_export]
macro_rules! define_discriminator {
    ($t:ty, $v:expr) => {
        paste::paste! {
            const [<$t:snake:upper _DISCRIMINATOR>]: &[u8] = $v;

            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<$t:snake _discriminator_equality_test>] {
                #[test]
                #[allow(non_snake_case)]
                fn [<it_tests_ $t _discriminator_equality>]() {
                    assert_eq!($v, &$crate::utils::anchor_event_discriminator(stringify!($t)));
                }
            }
        }
    };
}

pub const ANCHOR_DISCRIMINATOR_BYTE_SIZE: usize = 8;

pub fn find_program_address_with_u64_nonce(
    seeds: &[&[u8]],
    program_id: &Pubkey,
) -> anyhow::Result<(Pubkey, u64)> {
    let mut bump_seed = u64::MAX;
    for _ in 0..u8::MAX {
        {
            let mut seeds_with_bump = seeds.to_vec();
            seeds_with_bump.push(bytes_of(&bump_seed));
            match Pubkey::create_program_address(&seeds_with_bump, program_id) {
                Ok(address) => return Ok((address, bump_seed)),
                Err(PubkeyError::InvalidSeeds) => (),
                _ => break,
            }
        }
        bump_seed -= 1;
    }
    anyhow::bail!("unable to find valid program address for seed {seeds:?}")
}
