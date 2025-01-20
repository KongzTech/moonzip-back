use super::misc::{Balance, StoredPubkey};
use chrono::DateTime;
use moonzip::project::{CurvePoolVariant, ProjectSchema};
use serde::{Deserialize, Serialize};
use services_common::TZ;
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
    pub static_pool_mint: Option<StoredPubkey>,
    pub curve_pool_mint: Option<StoredPubkey>,
    pub created_at: DateTime<TZ>,
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

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "curve_variant")]
pub enum CurveVariant {
    Moonzip,
    Pumpfun,
}
