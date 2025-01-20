use std::{fmt::Display, future::Future};

use anyhow::bail;
use reqwest::Response;
use serde::{de::DeserializeOwned, Serialize, Serializer};
use tracing::debug;

pub mod keypair;
pub mod limiter;

/// Decodes type from json or return err with raw body info.
pub async fn decode_response_type_or_raw<T: DeserializeOwned>(
    response: Response,
) -> anyhow::Result<T> {
    let status = response.status();
    let bytes = response.bytes().await?;
    match serde_json::from_slice::<T>(&bytes) {
        Ok(response) => Ok(response),
        Err(err) => {
            bail!(
                "failed to decode to json response: {err:?}, raw body: {:?}, status: {status}",
                String::from_utf8(bytes.to_vec())
            )
        }
    }
}

pub fn decode_type_or_raw<T: DeserializeOwned>(data: impl AsRef<[u8]>) -> anyhow::Result<T> {
    let bytes = data.as_ref();
    match serde_json::from_slice::<T>(bytes) {
        Ok(response) => Ok(response),
        Err(err) => {
            bail!(
                "failed to decode to json response: {err:?}, raw body: {:?}",
                String::from_utf8(bytes.to_vec())
            )
        }
    }
}

pub fn as_anyhow<E: Display>(err: E) -> anyhow::Error {
    anyhow::anyhow!("{err}")
}

pub mod serde_timestamp {
    use chrono::DateTime;
    use serde::{de, Deserialize, Deserializer, Serializer};

    use crate::TZ;

    pub fn serialize<S>(dt: &DateTime<TZ>, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serializer.serialize_u64(dt.timestamp() as u64)
    }

    pub fn deserialize<'de, D>(deserializer: D) -> Result<DateTime<TZ>, D::Error>
    where
        D: Deserializer<'de>,
    {
        let raw = u64::deserialize(deserializer)?;
        DateTime::from_timestamp(raw as i64, 0).ok_or_else(|| de::Error::custom("invalid datetime"))
    }
}

pub async fn repeat_until_ok<
    T,
    E: Display + Sync + Send + 'static,
    F: Future<Output = Result<T, E>>,
    FN: Fn() -> F,
>(
    new_fut: FN,
    max_repeats: u64,
) -> anyhow::Result<T> {
    let mut repeats = 0;
    while repeats < max_repeats {
        let iter_res = new_fut().await;
        match iter_res {
            Ok(res) => return Ok(res),
            Err(err) => {
                debug!("repeated future completed with err at {repeats}: {err:#}")
            }
        }
        repeats += 1;
    }
    bail!("future iterated {max_repeats} without success")
}

pub fn serialize_tx_bs58<S>(tx: &impl Serialize, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    let serialized_data =
        bincode::serialize(tx).map_err(|err| serde::ser::Error::custom(err.to_string()))?;
    serializer.serialize_str(&bs58::encode(serialized_data).into_string())
}
