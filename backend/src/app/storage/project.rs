use super::misc::{Balance, StoredKeypair, StoredPubkey};
use chrono::DateTime;
use moonzip::project::{CurvePoolVariant, ProjectSchema};
use serde::{Deserialize, Serialize};
use services_common::TZ;
use solana_sdk::{pubkey::Pubkey, signer::Signer};
use sqlx::types::Uuid;
use validator::Validate;

pub fn project_id(id: &Uuid) -> moonzip::project::ProjectId {
    moonzip::project::ProjectId::from(id.to_u128_le())
}

pub type ProjectId = Uuid;

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct StoredProject {
    pub id: ProjectId,
    pub owner: StoredPubkey,
    pub deploy_schema: DeploySchema,
    pub stage: Stage,
    pub static_pool_pubkey: Option<StoredPubkey>,
    pub curve_pool_keypair: Option<StoredKeypair>,
    pub created_at: DateTime<TZ>,
}

impl StoredProject {
    pub fn static_pool_mint(&self) -> Option<Pubkey> {
        self.static_pool_pubkey.as_ref().map(|key| key.to_pubkey())
    }

    pub fn curve_pool_mint(&self) -> Option<Pubkey> {
        self.curve_pool_keypair
            .as_ref()
            .map(|key| key.to_keypair().pubkey())
    }
}

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, Clone, Copy, PartialEq, Eq, PartialOrd, Ord,
)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "project_stage")]
pub enum Stage {
    Created,
    OnStaticPool,
    StaticPoolClosed,
    OnCurvePool,
    CurvePoolClosed,
    Graduated,
}

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct StoredTokenMeta {
    pub project_id: ProjectId,
    pub name: String,
    pub symbol: String,
    pub description: String,
    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
    pub deployed_url: Option<String>,
}

impl StoredTokenMeta {
    pub fn deployed_url(&self) -> anyhow::Result<String> {
        self.deployed_url
            .as_ref()
            .cloned()
            .ok_or_else(|| anyhow::anyhow!("invariant: token meta is not deployed"))
    }
}

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct StoredTokenImage {
    pub project_id: ProjectId,
    pub image_content: Vec<u8>,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type, Validate)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "deploy_schema")]
pub struct DeploySchema {
    pub use_static_pool: bool,
    pub curve_pool: CurveVariant,
    #[validate(range(min = 0))]
    pub launch_after: i64,
    pub dev_purchase: Option<Balance>,
}

impl DeploySchema {
    pub fn to_project_schema(&self) -> ProjectSchema {
        ProjectSchema {
            use_static_pool: self.use_static_pool,
            curve_pool: match self.curve_pool {
                CurveVariant::Moonzip => CurvePoolVariant::Moonzip,
                CurveVariant::Pumpfun => CurvePoolVariant::Pumpfun,
            },
            dev_purchase: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, sqlx::Type)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "curve_variant")]
pub enum CurveVariant {
    Moonzip,
    Pumpfun,
}
