use serde::{Deserialize, Serialize};
use std::sync::Arc;
use utoipa::{IntoParams, ToSchema};

const OWNER_ADDRESS_PARAM: &str = "owner-address";

pub struct HeliusClient {
    client: reqwest::Client,
    config: Arc<HeliusClientConfig>,
}

impl HeliusClient {
    pub fn new(config: HeliusClientConfig) -> Self {
        Self {
            client: reqwest::Client::new(),
            config: Arc::new(config),
        }
    }

    pub fn generate_url(&self, owner_address: String) -> String {
        format!(
            "{}?{}={}&{}={}",
            self.config.client.url,
            API_KEY_PARAM,
            self.config.client.api_key,
            OWNER_ADDRESS_PARAM,
            owner_address
        )
    }
    pub async fn get_owned_nfts(
        &self,
        owner_address: String,
        page: Option<u32>,
        limit: Option<u32>,
    ) -> anyhow::Result<GetOwnedNFTsResponse> {
        let url = self.generate_url(owner_address.clone());

        let request = GetOwnedNFTsPayload {
            jsonrpc: String::from(JSONRPC_VERSION),
            id: String::from(SAMPLE_ID),
            method: String::from(HeliusDASMethod::GetAssetsByOwner),
            params: Option::from(GetOwnedNFTsRequestParams {
                owner_address,
                page,
                limit,
                sort_by: Option::from(SortBy {
                    sort_by: String::from(HeliusSortBy::RecentAction),
                    sort_direction: String::from(HeliusSortDirection::DESC),
                }),
                options: Some(RequestOptions {
                    show_unverified_collections: false,
                    show_collection_metadata: false,
                    show_grand_total: false,
                    show_fungible: false,
                    show_native_balance: false,
                    show_inscription: false,
                    show_zero_balance: false,
                }),
            }),
        };

        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await?
            .json::<GetOwnedNFTsResponse>()
            .await?;

        Ok(response)
    }

    pub async fn get_nft_info(
        &self,
        nft_address: String,
        owner_address: String,
    ) -> anyhow::Result<GetAssetNFTsResponse> {
        let url = self.generate_url(owner_address);
        let request = GetNFTsAssetPayload {
            jsonrpc: String::from(JSONRPC_VERSION),
            id: String::from(SAMPLE_ID),
            method: String::from(HeliusDASMethod::GetAsset),
            params: Option::from(GetNFTsAssetParams { id: nft_address }),
        };

        let response = self
            .client
            .post(url)
            .json(&request)
            .send()
            .await?
            .json::<GetAssetNFTsResponse>()
            .await?;

        Ok(response)
    }
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeliusClientConfig {
    pub client: HeliusClientItemConfig,
}

#[derive(Debug, Deserialize, Clone)]
pub struct HeliusClientItemConfig {
    pub url: String,
    pub api_key: String,
}

impl HeliusClientItemConfig {
    pub fn from_cfg(cfg: HeliusClientItemConfig) -> Self {
        Self {
            url: cfg.url,
            api_key: cfg.api_key,
        }
    }
}

pub const API_KEY_PARAM: &str = "api-key";
pub const JSONRPC_VERSION: &str = "2.0";
pub const SAMPLE_ID: &str = "text";

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HeliusDASMethod {
    GetAssetsByOwner,
    GetAsset,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "snake_case")]
pub enum HeliusSortBy {
    RecentAction,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub enum HeliusSortDirection {
    ASC,
    DESC,
}

impl From<HeliusDASMethod> for String {
    fn from(method: HeliusDASMethod) -> Self {
        match method {
            HeliusDASMethod::GetAssetsByOwner => "getAssetsByOwner".to_string(),
            HeliusDASMethod::GetAsset => "getAsset".to_string(),
        }
    }
}

impl From<HeliusSortBy> for String {
    fn from(method: HeliusSortBy) -> Self {
        match method {
            HeliusSortBy::RecentAction => "recent_action".to_string(),
        }
    }
}

impl From<HeliusSortDirection> for String {
    fn from(method: HeliusSortDirection) -> Self {
        match method {
            HeliusSortDirection::ASC => "asc".to_string(),
            HeliusSortDirection::DESC => "desc".to_string(),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetOwnedNFTsPayload {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Option<GetOwnedNFTsRequestParams>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetNFTsAssetPayload {
    pub jsonrpc: String,
    pub id: String,
    pub method: String,
    pub params: Option<GetNFTsAssetParams>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetNFTsAssetParams {
    pub id: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetNFTAssetResponseDetail {
    pub interface: Option<String>,
    pub id: Option<String>,
    pub content: NFTContent,
    pub authorities: Vec<Authority>,
    pub compression: Compression,
    pub grouping: Vec<Grouping>,
    pub royalty: Royalty,
    pub creators: Vec<Creator>,
    pub ownership: Ownership,
    pub supply: Supply,
    pub mutable: Option<bool>,
    pub burnt: Option<bool>,
    pub token_info: Option<TokenInfo>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetAssetNFTsResponse {
    pub jsonrpc: Option<String>,
    pub result: Option<GetNFTAssetResponseDetail>,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetOwnedNFTsRequestParams {
    pub owner_address: String,
    pub page: Option<u32>,
    pub limit: Option<u32>,
    pub sort_by: Option<SortBy>,
    pub options: Option<RequestOptions>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SortBy {
    pub sort_by: String,
    pub sort_direction: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct RequestOptions {
    pub show_unverified_collections: bool,
    pub show_collection_metadata: bool,
    pub show_grand_total: bool,
    pub show_fungible: bool,
    pub show_native_balance: bool,
    pub show_inscription: bool,
    pub show_zero_balance: bool,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetOwnedNFTsResponse {
    pub jsonrpc: Option<String>,
    pub result: Option<GetOwnedNFTsResponseDetail>,
    pub id: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetOwnedNFTsResponseDetail {
    pub total: u64,
    pub limit: u64,
    pub page: u64,
    pub items: Vec<NFTItem>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NFTItem {
    pub interface: Option<String>,
    pub id: Option<String>,
    pub content: NFTContent,
    pub authorities: Vec<Authority>,
    pub compression: Compression,
    pub grouping: Vec<Grouping>,
    pub royalty: Royalty,
    pub creators: Vec<Creator>,
    pub ownership: Ownership,
    pub supply: Supply,
    pub mutable: Option<bool>,
    pub burnt: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NFTContent {
    pub json_uri: Option<String>,
    pub files: Vec<NFTFile>,
    pub metadata: Metadata,
    pub links: Links,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct NFTFile {
    pub uri: Option<String>,
    pub cdn_uri: Option<String>,
    pub mime: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Metadata {
    pub description: Option<String>,
    pub name: Option<String>,
    pub symbol: Option<String>,
    pub token_standard: Option<String>,
    pub attributes: Option<Vec<Attribute>>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Attribute {
    pub value: Option<String>,
    pub trait_type: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Links {
    pub external_url: Option<String>,
    pub image: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Authority {
    pub address: Option<String>,
    pub scopes: Vec<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Compression {
    pub eligible: Option<bool>,
    pub compressed: Option<bool>,
    pub data_hash: Option<String>,
    pub creator_hash: Option<String>,
    pub asset_hash: Option<String>,
    pub tree: Option<String>,
    pub seq: Option<u64>,
    pub leaf_id: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Grouping {
    pub group_key: Option<String>,
    pub group_value: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Royalty {
    pub royalty_model: Option<String>,
    pub target: Option<String>,
    pub percent: Option<f64>,
    pub basis_points: Option<u64>,
    pub primary_sale_happened: Option<bool>,
    pub locked: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Creator {
    pub address: Option<String>,
    pub share: Option<u64>,
    pub verified: Option<bool>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Ownership {
    pub frozen: bool,
    pub delegated: bool,
    pub delegate: Option<String>,
    pub ownership_model: Option<String>,
    pub owner: Option<String>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct Supply {
    pub print_max_supply: Option<u64>,
    pub print_current_supply: Option<u64>,
    pub edition_nonce: Option<u64>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct TokenInfo {
    pub supply: Option<u32>,
    pub decimals: Option<u32>,
    pub token_program: Option<String>,
    pub mint_authority: Option<String>,
    pub freeze_authority: Option<String>,
}
