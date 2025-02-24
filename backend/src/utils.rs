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
