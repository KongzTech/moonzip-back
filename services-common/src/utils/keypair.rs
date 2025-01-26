use std::{fs::File, ops::Deref, path::PathBuf, sync::Arc};

use serde::{
    de::{self},
    Deserialize, Deserializer,
};
use solana_sdk::{signature::Keypair, signer::Signer};

#[derive(Deserialize, PartialEq)]
pub struct SaneKeypair(#[serde(deserialize_with = "deserialize_keypair")] Arc<Keypair>);

impl SaneKeypair {
    pub fn to_keypair(&self) -> Keypair {
        self.0.insecure_clone()
    }
}

impl Clone for SaneKeypair {
    fn clone(&self) -> Self {
        Self(self.0.clone())
    }
}

impl Deref for SaneKeypair {
    type Target = Keypair;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl From<SaneKeypair> for Keypair {
    fn from(val: SaneKeypair) -> Self {
        val.0.insecure_clone()
    }
}

impl From<Keypair> for SaneKeypair {
    fn from(value: Keypair) -> Self {
        Self(value.into())
    }
}

impl std::fmt::Debug for SaneKeypair {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.pubkey())
    }
}

pub fn deserialize_keypair<'de, D, K: From<Keypair>>(deserializer: D) -> Result<K, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(rename_all = "lowercase")]
    #[serde(untagged)]
    enum SerdeKeypair {
        FromFile { path: PathBuf },
        Raw { array: Vec<u8> },
    }

    let raw = SerdeKeypair::deserialize(deserializer)?;
    let bytes = match raw {
        SerdeKeypair::FromFile { path } => {
            serde_json::from_reader(File::open(path).map_err(de::Error::custom)?)
                .map_err(de::Error::custom)?
        }
        SerdeKeypair::Raw { array } => array,
    };
    Keypair::from_bytes(&bytes)
        .map_err(de::Error::custom)
        .map(Into::into)
}
