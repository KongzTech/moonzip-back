use anchor_client::anchor_lang::AnchorDeserialize;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use services_common::{
    solana::pool::SolanaPool,
    utils::{decode_response_type_or_raw, period_fetch::FetchExecutor},
};
use solana_program::pubkey;
use solana_sdk::{commitment_config::CommitmentConfig, pubkey::Pubkey};
use std::time::Duration;

pub static MINT_AUTHORITY: Lazy<Pubkey> = Lazy::new(get_mint_authority);
pub static GLOBAL: Lazy<Pubkey> = Lazy::new(get_global);
pub const EVENT_AUTHORITY: Pubkey = pubkey!("Ce6TQqeHC9p8KetsN6JsjHK7UTZk7nasjjnr7XxXp9F1");

fn get_mint_authority() -> Pubkey {
    Pubkey::find_program_address(&[b"mint-authority"], &pumpfun_cpi::ID).0
}

fn get_global() -> Pubkey {
    Pubkey::find_program_address(&[b"global"], &pumpfun_cpi::ID).0
}

pub fn get_bonding_curve(mint: &Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[b"bonding-curve", mint.as_ref()], &pumpfun_cpi::ID).0
}

#[derive(Debug, Clone)]
pub struct CreateTokenMetadata {
    /// Name of the token
    pub name: String,
    /// Token symbol (e.g. "BTC")
    pub symbol: String,
    /// Description of the token
    pub description: String,
    pub image_content: Vec<u8>,
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

pub struct HttpClient {
    client: reqwest::Client,
}

impl Default for HttpClient {
    fn default() -> Self {
        Self::new()
    }
}

impl HttpClient {
    pub fn new() -> Self {
        Self {
            client: reqwest::Client::new(),
        }
    }

    pub async fn deploy_metadata(
        &self,
        metadata: CreateTokenMetadata,
    ) -> anyhow::Result<TokenMetadataResponse> {
        let boundary = "------------------------f4d9c2e8b7a5310f";
        let mut body = Vec::new();

        // Helper function to append form data
        fn append_text_field(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
            body.extend_from_slice(b"--");
            body.extend_from_slice(boundary.as_bytes());
            body.extend_from_slice(b"\r\n");
            body.extend_from_slice(
                format!("Content-Disposition: form-data; name=\"{}\"\r\n\r\n", name).as_bytes(),
            );
            body.extend_from_slice(value.as_bytes());
            body.extend_from_slice(b"\r\n");
        }

        // Append form fields
        append_text_field(&mut body, boundary, "name", &metadata.name);
        append_text_field(&mut body, boundary, "symbol", &metadata.symbol);
        append_text_field(&mut body, boundary, "description", &metadata.description);
        if let Some(twitter) = metadata.twitter {
            append_text_field(&mut body, boundary, "twitter", &twitter);
        }
        if let Some(telegram) = metadata.telegram {
            append_text_field(&mut body, boundary, "telegram", &telegram);
        }
        if let Some(website) = metadata.website {
            append_text_field(&mut body, boundary, "website", &website);
        }
        append_text_field(&mut body, boundary, "showName", "true");

        // Append file part
        body.extend_from_slice(b"--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"\r\n");
        body.extend_from_slice(
            b"Content-Disposition: form-data; name=\"file\"; filename=\"file\"\r\n",
        );
        body.extend_from_slice(b"Content-Type: application/octet-stream\r\n\r\n");

        // Read the file contents
        body.extend_from_slice(&metadata.image_content);

        // Close the boundary
        body.extend_from_slice(b"\r\n--");
        body.extend_from_slice(boundary.as_bytes());
        body.extend_from_slice(b"--\r\n");

        let response = self
            .client
            .post("https://pump.fun/api/ipfs")
            .header(
                "Content-Type",
                format!("multipart/form-data; boundary={}", boundary),
            )
            .header("Content-Length", body.len() as u64)
            .body(body)
            .timeout(Duration::from_secs(7))
            .send()
            .await?;
        let json = decode_response_type_or_raw(response).await?;
        Ok(json)
    }
}

pub struct MetaFetcher {
    pub pool: SolanaPool,
}

#[async_trait::async_trait]
impl FetchExecutor<Meta> for MetaFetcher {
    fn name(&self) -> &'static str {
        "pumpfun-meta"
    }

    async fn init(&mut self) -> anyhow::Result<()> {
        Ok(())
    }

    async fn fetch(&mut self) -> anyhow::Result<Meta> {
        let client = self.pool.rpc_client().use_single().await;
        let global = client
            .get_account_with_commitment(&GLOBAL, CommitmentConfig::finalized())
            .await?;
        let marker = global.context.slot;
        Ok(Meta {
            marker,
            global_account: pumpfun_cpi::Global::try_from_slice(
                &global
                    .value
                    .ok_or_else(|| anyhow::anyhow!("no global curve pool account yet"))?
                    .data,
            )?,
        })
    }
}

#[derive(Clone)]
pub struct Meta {
    pub marker: u64,
    pub global_account: pumpfun_cpi::Global,
}

impl PartialEq for Meta {
    fn eq(&self, other: &Self) -> bool {
        self.marker == other.marker
    }
}

impl PartialOrd for Meta {
    fn partial_cmp(&self, other: &Self) -> Option<std::cmp::Ordering> {
        self.marker.partial_cmp(&other.marker)
    }
}
