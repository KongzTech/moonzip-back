use crate::app::storage::project::ImageStream;
use derive_more::Into;

use serde::Deserialize;
use serde::Serialize;
use serde_json::json;
use services_common::utils::decode_response_type_or_raw;
use std::{sync::Arc, time::Duration};

use reqwest::multipart::Form;

use http::HeaderMap;
use http::HeaderValue;
use reqwest::multipart::Part;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct IpfsClientConfig {
    pub api_key: String,
    pub gateway: String,
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

pub fn default_base_url() -> String {
    "https://api.pinata.cloud".to_string()
}

#[derive(Debug, Clone)]
pub struct IpfsClient {
    client: reqwest::Client,
    config: Arc<IpfsClientConfig>,
}

const PIN_ENDPOINT: &str = "/pinning/pinFileToIPFS";

const TEST_AUTH_ENDPOINT: &str = "/data/testAuthentication";

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
        Ok(Self {
            client,
            config: Arc::new(config),
        })
    }

    pub async fn verify_connection(&self) -> anyhow::Result<()> {
        let response = self
            .client
            .get(format!("{}{}", self.config.base_url, TEST_AUTH_ENDPOINT))
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

    pub async fn upload_image(
        &self,
        image_content: ImageStream<'_>,
        name: &str,
    ) -> anyhow::Result<String> {
        let form = Form::new()
            .part(
                "file",
                Part::stream(image_content)
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

        let endpoint = format!("{}{}", self.config.base_url, PIN_ENDPOINT);

        let response = self
            .client
            .post(endpoint)
            .multipart(form)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        let result = decode_response_type_or_raw::<PinResult>(response).await?;
        Ok(ipfs_url(&self.config.gateway, &result.ipfs_hash))
    }

    pub async fn upload_json(&self, json: impl Serialize, name: &str) -> anyhow::Result<String> {
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

        let endpoint = format!("{}{}", self.config.base_url, PIN_ENDPOINT);

        let response = self
            .client
            .post(endpoint)
            .multipart(form)
            .timeout(Duration::from_secs(10))
            .send()
            .await?;

        let result = decode_response_type_or_raw::<PinResult>(response).await?;
        Ok(ipfs_url(&self.config.gateway, &result.ipfs_hash))
    }
}

#[derive(Debug, Deserialize, Clone)]
pub(crate) struct PinResult {
    #[serde(rename = "IpfsHash")]
    pub(crate) ipfs_hash: String,
}

pub fn ipfs_url(gateway_tag: &str, ipfs_hash: &str) -> String {
    format!("https://{}.mypinata.cloud/ipfs/{}", gateway_tag, ipfs_hash)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use services_common::utils::decode_response_type_or_raw;
    use std::env;
    use std::path::PathBuf;

    async fn client() -> IpfsClient {
        let client = IpfsClient::new(IpfsClientConfig {
            api_key: env::var("PINATA_API_KEY").unwrap(),
            gateway: env::var("PINATA_GATEWAY").unwrap(),
            base_url: default_base_url(),
        })
        .unwrap();
        client.verify_connection().await.unwrap();
        client
    }

    #[tokio::test]
    #[ignore = "goes to the internet"]
    async fn test_pin_image() -> anyhow::Result<()> {
        let mut path = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
        path.push("../tests/data/moon.png");

        let client = client().await;

        let image_content = ImageStream::from_file(&path)?;
        let result = client.upload_image(image_content, "moon").await?;
        println!("{:?}", result);
        let response = reqwest::get(result).await;
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

        let initial_json: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string("../tests/data/metadata.json")?)?;
        let result = client.upload_json(&initial_json, "something").await?;
        println!("{:?}", result);

        tokio::time::sleep(Duration::from_secs(3)).await;
        let response = reqwest::get(result).await?;

        let json: serde_json::Value = decode_response_type_or_raw(response).await?;
        assert_eq!(json, initial_json);

        Ok(())
    }
}
