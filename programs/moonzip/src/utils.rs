use anchor_lang::AccountSerialize;
use num::Bounded;

pub trait Sizable {
    fn longest() -> Self;
}

impl<T: Bounded> Sizable for T {
    fn longest() -> Self {
        Self::max_value()
    }
}

pub fn assert_type_equal<T: Sizable + AccountSerialize>(size: usize) {
    let mut result: Vec<u8> = Vec::new();
    T::longest().try_serialize(&mut result).unwrap();
    let serialized_size = result.len();

    assert!(
        serialized_size == size,
        "set account size({}) must match actual serialization size({})",
        size,
        serialized_size
    )
}

#[macro_export]
macro_rules! ensure_account_size {
    ($t:ty, $s:expr) => {
        impl $t {
            pub const ACCOUNT_SIZE: usize = $s;
        }

        paste::paste! {
            #[cfg(test)]
            #[allow(non_snake_case)]
            mod [<$t _size_test>] {
                use super::$t;

                #[test]
                #[allow(non_snake_case)]
                fn [<it_tests_ $t _account_size>]() {
                    $crate::utils::assert_type_equal::<$t>($t::ACCOUNT_SIZE);
                }
            }
        }
    };
}
