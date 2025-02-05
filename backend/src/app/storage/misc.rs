use derive_more::derive::{From, Into};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, signature::Keypair};
#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, From, Into, Clone, PartialEq, Eq, PartialOrd, Ord,
)]
#[sqlx(transparent, type_name = "pubkey")]
pub struct StoredPubkey(Vec<u8>);

impl StoredPubkey {
    pub fn to_pubkey(&self) -> Pubkey {
        Pubkey::try_from(self.0.as_slice()).expect("invariant: invalid stored pubkey")
    }
}

impl From<Pubkey> for StoredPubkey {
    fn from(value: Pubkey) -> Self {
        Self(value.to_bytes().to_vec())
    }
}

impl From<StoredPubkey> for Pubkey {
    fn from(value: StoredPubkey) -> Self {
        Pubkey::try_from(value.0.as_slice()).expect("invariant: invalid stored pubkey")
    }
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, From, Into, Clone, PartialEq, Eq, PartialOrd, Ord,
)]
#[sqlx(transparent, type_name = "keypair")]
pub struct StoredKeypair(Vec<u8>);

impl StoredKeypair {
    pub fn to_keypair(&self) -> Keypair {
        Keypair::from_bytes(self.0.as_slice()).expect("invariant: invalid stored secret key")
    }

    pub fn from_keypair(keypair: &Keypair) -> Self {
        Self(keypair.to_bytes().to_vec())
    }
}

impl From<Keypair> for StoredKeypair {
    fn from(value: Keypair) -> Self {
        Self(value.to_bytes().to_vec())
    }
}

impl From<StoredKeypair> for Keypair {
    fn from(value: StoredKeypair) -> Self {
        Keypair::from_bytes(value.0.as_slice()).expect("invariant: invalid stored keypair")
    }
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, From, Into, Clone, PartialEq, Eq, PartialOrd, Ord,
)]
#[sqlx(type_name = "balance")]
pub struct Balance(Decimal);

impl From<u64> for Balance {
    fn from(value: u64) -> Self {
        Self(Decimal::from(value))
    }
}

impl TryFrom<Balance> for u64 {
    type Error = anyhow::Error;

    fn try_from(value: Balance) -> Result<Self, Self::Error> {
        Ok(value.0.try_into()?)
    }
}
