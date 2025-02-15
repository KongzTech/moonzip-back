use crate::app::storage::project::ImageStream;
use reqwest::multipart::{Form, Part};
use serde::{Deserialize, Serialize};
use services_common::utils::decode_response_type_or_raw;
use std::{sync::Arc, time::Duration};

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct PumpfunIpfsClientConfig {
    #[serde(default = "default_base_url")]
    pub base_url: String,
}

fn default_base_url() -> String {
    "https://pump.fun".to_string()
}

pub struct CreateTokenMetadata<'a> {
    /// Name of the token
    pub name: String,
    /// Token symbol (e.g. "BTC")
    pub symbol: String,
    /// Description of the token
    pub description: String,
    pub image_content: ImageStream<'a>,
    /// Optional Twitter handle
    pub twitter: Option<String>,
    /// Optional Telegram group
    pub telegram: Option<String>,
    /// Optional website URL
    pub website: Option<String>,
}

/// Response received after successfully uploading token metadata.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TokenMetadataResponse {
    /// IPFS URI where the metadata is stored
    pub metadata_uri: String,
}

pub struct PumpfunIpfsClient {
    client: reqwest::Client,
    config: Arc<PumpfunIpfsClientConfig>,
}

impl PumpfunIpfsClient {
    pub fn new(config: PumpfunIpfsClientConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: Arc::new(config),
        }
    }

    pub async fn deploy_metadata(
        &self,
        metadata: CreateTokenMetadata<'_>,
    ) -> anyhow::Result<TokenMetadataResponse> {
        // Create a multipart form
        let mut form = Form::new()
            .text("name", metadata.name)
            .text("symbol", metadata.symbol)
            .text("description", metadata.description)
            .text("showName", "true");

        // Add optional fields if they exist
        if let Some(twitter) = metadata.twitter {
            form = form.text("twitter", twitter);
        }
        if let Some(telegram) = metadata.telegram {
            form = form.text("telegram", telegram);
        }
        if let Some(website) = metadata.website {
            form = form.text("website", website);
        }

        // Add the image file part
        let image_part = Part::stream(metadata.image_content)
            .file_name("file")
            .mime_str("application/octet-stream")?;
        form = form.part("file", image_part);

        // Send the request
        let response = self
            .client
            .post(format!("{}/api/ipfs", self.config.base_url))
            .multipart(form)
            .timeout(Duration::from_secs(7))
            .send()
            .await?;

        // Decode the response
        let json = decode_response_type_or_raw(response).await?;
        Ok(json)
    }
}
