use anchor_client::anchor_lang::AnchorDeserialize;
use once_cell::sync::Lazy;
use serde::{Deserialize, Serialize};
use services_common::{solana::pool::SolanaPool, utils::decode_response_type_or_raw};
use solana_program::pubkey;
use solana_sdk::pubkey::Pubkey;
use std::time::Duration;
use tokio::sync::watch;
use tracing::error;

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

#[derive(Clone)]
pub struct MetaReceiver(watch::Receiver<Option<Meta>>);

impl MetaReceiver {
    pub async fn get(&mut self) -> anyhow::Result<Meta> {
        Ok(self
            .0
            .wait_for(|meta| meta.is_some())
            .await?
            .as_ref()
            .map(|meta| meta.clone())
            .expect("invariant: waited for some"))
    }
}

pub struct MetaFetcher {
    pool: SolanaPool,
}

static META_TICK_INTERVAL: Lazy<Duration> = Lazy::new(|| Duration::from_secs(60) * 60 * 12);
static META_ERROR_BACKOFF: Lazy<Duration> = Lazy::new(|| Duration::from_secs(3));

impl MetaFetcher {
    pub fn new(pool: SolanaPool) -> Self {
        Self { pool }
    }

    pub fn serve(self) -> MetaReceiver {
        let (sender, receiver) = watch::channel(None);
        tokio::spawn(async move {
            loop {
                match self.tick(&sender).await {
                    Ok(_) => {
                        tokio::time::sleep(*META_TICK_INTERVAL).await;
                    }
                    Err(e) => {
                        error!("failed to fetch pumpfun meta: {}", e);
                        tokio::time::sleep(*META_ERROR_BACKOFF).await;
                    }
                }
            }
        });
        MetaReceiver(receiver)
    }

    async fn tick(&self, sender: &watch::Sender<Option<Meta>>) -> anyhow::Result<()> {
        let meta = self.fetch().await?;
        sender.send(Some(meta))?;
        Ok(())
    }

    async fn fetch(&self) -> anyhow::Result<Meta> {
        let client = self.pool.client().rpc().use_single().await;
        let global = client.get_account_data(&GLOBAL).await?;
        Ok(Meta {
            global_account: pumpfun_cpi::Global::try_from_slice(&global)?,
        })
    }
}

#[derive(Clone)]
pub struct Meta {
    pub global_account: pumpfun_cpi::Global,
}
