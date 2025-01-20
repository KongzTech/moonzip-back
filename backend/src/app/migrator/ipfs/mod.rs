use derive_more::Into;
use http::{HeaderMap, HeaderValue};
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use serde_json::json;
use services_common::utils::decode_response_type_or_raw;
use std::time::Duration;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IpfsClientConfig {
    pub api_key: String,
}

#[derive(Debug, Clone)]
pub struct IpfsClient {
    client: reqwest::Client,
}

const PIN_ENDPOINT: &str = "https://api.pinata.cloud/pinning/pinFileToIPFS";
const TEST_AUTH_ENDPOINT: &str = "https://api.pinata.cloud/data/testAuthentication";

impl IpfsClient {
    pub fn new(config: IpfsClientConfig) -> anyhow::Result<Self> {
        let mut headers = HeaderMap::new();
        headers.insert(
            "Authorization",
            HeaderValue::from_str(&format!("Bearer {}", config.api_key))?,
        );
        let client = reqwest::Client::builder()
            .default_headers(headers)
            .build()?;
        Ok(Self { client })
    }

    pub async fn verify_connection(&self) -> anyhow::Result<()> {
        let response = self
            .client
            .get(TEST_AUTH_ENDPOINT)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;
        if !response.status().is_success() {
            anyhow::bail!(
                "failed to verify connection to pinata: {}",
                response.status()
            );
        }
        Ok(())
    }

    pub async fn pin_image(&self, image_content: Vec<u8>, name: &str) -> anyhow::Result<PinResult> {
        let form = Form::new()
            .part(
                "file",
                Part::bytes(image_content)
                    .file_name(format!("{}.png", name))
                    .mime_str("image/png")?,
            )
            .part(
                "pinataMetadata",
                Part::bytes(serde_json::to_vec(&json!({
                    "name": name
                }))?)
                .mime_str("application/json")?,
            );

        let response = self
            .client
            .post(PIN_ENDPOINT)
            .multipart(form)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        decode_response_type_or_raw::<PinResult>(response).await
    }

    pub async fn pin_json(&self, json: impl Serialize, name: &str) -> anyhow::Result<PinResult> {
        let json_content = serde_json::to_vec(&json)?;
        let form = Form::new()
            .part(
                "file",
                Part::bytes(json_content)
                    .file_name(format!("{}.json", name))
                    .mime_str("application/json")?,
            )
            .part(
                "pinataMetadata",
                Part::bytes(serde_json::to_vec(&json!({
                    "name": name
                }))?)
                .mime_str("application/json")?,
            );

        let response = self
            .client
            .post(PIN_ENDPOINT)
            .multipart(form)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        decode_response_type_or_raw::<PinResult>(response).await
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct PinResult {
    #[serde(rename = "IpfsHash")]
    pub ipfs_hash: String,
}

pub fn ipfs_url(gateway_tag: &str, ipfs_hash: &str) -> String {
    format!("https://{}.mypinata.cloud/ipfs/{}", gateway_tag, ipfs_hash)
}

#[cfg(test)]
mod tests {
    use super::*;
    use services_common::utils::decode_response_type_or_raw;
    use std::env;
    use std::path::PathBuf;

    async fn client() -> IpfsClient {
        let client = IpfsClient::new(IpfsClientConfig {
            api_key: env::var("PINATA_API_KEY").unwrap(),
        })
        .unwrap();
        client.verify_connection().await.unwrap();
        client
    }

    fn gateway() -> String {
        env::var("PINATA_GATEWAY").unwrap()
    }

    #[tokio::test]
    #[ignore = "goes to the internet"]
    async fn test_pin_image() -> anyhow::Result<()> {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../tests/data/moon.png");

        let client = client().await;

        let image_content = std::fs::read(path)?;
        let result = client.pin_image(image_content.clone(), "moon").await?;
        println!("{:?}", result);
        let response = reqwest::get(ipfs_url(&gateway(), &result.ipfs_hash)).await;
        assert!(
            response.is_ok(),
            "failed to get image from ipfs: {}",
            response.err().unwrap()
        );

        Ok(())
    }

    #[tokio::test]
    #[ignore = "goes to the internet"]
    async fn test_pin_json() -> anyhow::Result<()> {
        let client = client().await;

        let json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string("../tests/data/metadata.json")?)?;
        let result = client.pin_json(&json, "something").await?;
        println!("{:?}", result);

        let url = ipfs_url(&gateway(), &result.ipfs_hash);
        println!("{}", url);

        tokio::time::sleep(Duration::from_secs(3)).await;
        let response = reqwest::get(&url).await?;

        let json: serde_json::Value = decode_response_type_or_raw(response).await?;
        assert_eq!(json["name"], "SOMETHING");

        Ok(())
    }
}
