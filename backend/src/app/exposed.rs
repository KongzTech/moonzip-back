use super::storage::{
    self,
    misc::{StoredKeypair, StoredPubkey},
    project::{CurveVariant, Stage, StoredDeploySchema, StoredStaticPoolConfig},
};
use anyhow::bail;
use chrono::DateTime;
use rust_decimal::prelude::Zero;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use services_common::{utils::serialize_tx_bs64, TZ};
use solana_sdk::{pubkey::Pubkey, signer::Signer as _, transaction::Transaction};
use std::time::Duration;
use storage::user_info::StoredUserInfo;
use tokio::io::AsyncRead;
use utoipa::{IntoParams, ToSchema};
use uuid::Uuid;
use validator::Validate;

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct PublicProject {
    pub id: Uuid,
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub owner: Pubkey,
    pub name: String,
    pub description: String,
    pub created_at: String,
    pub stage: PublicProjectStage,

    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub static_pool_mint: Option<Pubkey>,

    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub curve_pool_mint: Option<Pubkey>,

    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = Option<String>)]
    pub dev_lock_base: Option<Pubkey>,
}

pub struct StoredProjectInfo {
    pub id: Uuid,
    pub owner: StoredPubkey,
    pub name: String,
    pub description: String,
    pub stage: Stage,
    pub static_pool_pubkey: Option<StoredPubkey>,
    pub curve_pool_keypair: Option<StoredKeypair>,
    pub dev_lock_keypair: Option<StoredKeypair>,
    pub created_at: DateTime<TZ>,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UserInfo {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub wallet_address: Pubkey,
    pub username: String,
    pub display_name: Option<String>,
    pub image_url: Option<String>,
    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = String)]
    pub nft_address: Option<Pubkey>,
    pub last_active: Option<i64>,
    pub created_at: String,
    pub updated_at: String,
}

impl TryFrom<StoredProjectInfo> for PublicProject {
    type Error = anyhow::Error;

    fn try_from(project: StoredProjectInfo) -> Result<Self, Self::Error> {
        let stage = PublicProjectStage::from_stored(project.stage);
        // Hide project if it's stage could not be exposed.
        let Some(stage) = stage else {
            bail!("project stage could not be exposed")
        };

        let static_pool_mint = project.static_pool_pubkey.map(|pubkey| pubkey.to_pubkey());
        let mut curve_pool_mint = project
            .curve_pool_keypair
            .map(|keypair| keypair.to_keypair().pubkey());
        if stage < PublicProjectStage::CurvePoolActive {
            curve_pool_mint = None;
        }

        Ok(PublicProject {
            id: project.id,
            owner: project.owner.to_pubkey(),
            name: project.name,
            description: project.description,
            stage,
            created_at: project.created_at.to_string(),
            static_pool_mint,
            curve_pool_mint,
            dev_lock_base: project
                .dev_lock_keypair
                .map(|key| key.to_keypair().pubkey()),
        })
    }
}

impl TryFrom<StoredUserInfo> for UserInfo {
    type Error = anyhow::Error;

    fn try_from(stored: StoredUserInfo) -> Result<Self, Self::Error> {
        Ok(UserInfo {
            wallet_address: stored.wallet_address.to_pubkey(),
            username: stored.username,
            display_name: stored.display_name,
            image_url: stored.image_url,
            nft_address: stored.nft_address.map(|pubkey| pubkey.to_pubkey()),
            last_active: stored.last_active,
            created_at: stored.created_at.unwrap().to_string(),
            updated_at: stored.updated_at.unwrap().to_string(),
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, PartialEq, Eq, PartialOrd, Ord)]
#[serde(rename_all = "camelCase")]
pub enum PublicProjectStage {
    StaticPoolActive,
    StaticPoolClosed,
    CurvePoolActive,
    CurvePoolClosed,
    Graduated,
}

impl PublicProjectStage {
    pub fn from_stored(stored: storage::project::Stage) -> Option<Self> {
        match stored {
            storage::project::Stage::Created => None,
            storage::project::Stage::Confirmed => None,
            storage::project::Stage::OnStaticPool => Some(Self::StaticPoolActive),
            storage::project::Stage::StaticPoolClosed => Some(Self::StaticPoolClosed),
            storage::project::Stage::OnCurvePool => Some(Self::CurvePoolActive),
            storage::project::Stage::CurvePoolClosed => Some(Self::CurvePoolClosed),
            storage::project::Stage::Graduated => Some(Self::Graduated),
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectForm {
    pub request: CreateProjectRequest,
    #[schema(format = Binary, content_media_type = "application/octet-stream")]
    pub image_content: String,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub owner: Pubkey,
    pub meta: CreateTokenMeta,
    pub deploy_schema: DeploySchema,
}

pub struct CreateProjectStreamData<S: AsyncRead> {
    pub image_content: S,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct StaticPoolSchema {
    pub launch_period: u64,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DeploySchema {
    pub static_pool: Option<StaticPoolSchema>,
    pub curve_pool: CurveVariant,
    pub dev_purchase: Option<DevPurchase>,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DevPurchase {
    pub value: u64,
    pub lock: DevLockPeriod,
}

impl DeploySchema {
    pub fn try_to_stored(self) -> anyhow::Result<StoredDeploySchema> {
        let stored = StoredDeploySchema {
            static_pool: self
                .static_pool
                .map(|static_pool| {
                    Result::<_, anyhow::Error>::Ok(StoredStaticPoolConfig {
                        launch_ts: (TZ::now() + Duration::from_secs(static_pool.launch_period))
                            .timestamp(),
                    })
                })
                .transpose()?,
            curve_pool: self.curve_pool,
            dev_purchase: self.dev_purchase.map(|balance| balance.into()),
        };
        Ok(stored)
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateTokenMeta {
    pub name: String,
    pub symbol: String,
    pub description: String,

    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectResponse {
    pub project_id: Uuid,
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_tx_bs64")]
    pub transaction: Transaction,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BuyRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user: Pubkey,
    pub project_id: Uuid,
    pub sols: u64,
    pub min_token_output: Option<u64>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct BuyResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_tx_bs64")]
    pub transaction: Transaction,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SellRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user: Pubkey,
    pub project_id: Uuid,
    pub tokens: u64,
    pub min_sol_output: Option<u64>,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SellResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_tx_bs64")]
    pub transaction: Transaction,
}

#[serde_as]
#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DevLockClaimRequest {
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct DevLockClaimResponse {
    #[schema(value_type = String)]
    #[serde(serialize_with = "serialize_tx_bs64")]
    pub transaction: Transaction,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct SlippageSettings {
    #[schema(value_type = u16)]
    pub slippage_basis_points: u16,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema, IntoParams)]
#[serde(rename_all = "camelCase")]
pub struct GetProjectRequest {
    pub project_id: Uuid,
}

#[derive(Debug, Serialize, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetProjectResponse {
    pub project: Option<PublicProject>,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Clone, ToSchema, PartialEq, PartialOrd)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum DevLockPeriod {
    Disabled,
    Interval { interval: u64 },
}

impl DevLockPeriod {
    pub fn as_secs(&self) -> u64 {
        match self {
            DevLockPeriod::Disabled => 0,
            DevLockPeriod::Interval { interval } => *interval,
        }
    }

    pub fn from_secs(secs: u64) -> Self {
        if secs.is_zero() {
            return Self::Disabled;
        }
        Self::Interval { interval: secs }
    }
}

#[serde_as]
#[derive(Debug, Serialize, Validate, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct ChangeUserInfoRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub wallet_address: Pubkey,

    #[validate(length(min = 1, message = "Username cannot be empty"))]
    pub username: String,

    pub display_value: String,

    #[serde_as(as = "Option<DisplayFromStr>")]
    #[schema(value_type = String)]
    pub nft_address: Option<Pubkey>,
}

#[serde_as]
#[derive(Debug, Serialize, Validate, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetUserInformationRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub wallet_address: Pubkey,
}

#[serde_as]
#[derive(Debug, Serialize, Validate, Deserialize, Clone, ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct GetOwnedNFTsRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub owner_address: Pubkey,
    #[validate(range(
        min = 1,
        max = 1000,
        message = "Page must be greater than 0 and less than 1000"
    ))]
    pub page: Option<u32>,
    #[validate(range(
        min = 1,
        max = 1000,
        message = "Limit must be greater than 0 and less than 1000"
    ))]
    pub limit: Option<u32>,
}
