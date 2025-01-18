use crate::TZ;
use chrono::DateTime;
use derive_more::derive::{From, Into};
use moonzip::project::{CurvePoolVariant, ProjectId, ProjectSchema};
use serde::{Deserialize, Serialize};
use solana_sdk::pubkey::Pubkey;
use sqlx::types::Uuid;

pub fn project_id(id: &Uuid) -> ProjectId {
    ProjectId::from(id.to_u128_le())
}

pub fn project_address(id: &ProjectId) -> Pubkey {
    let (address, _) = Pubkey::find_program_address(&[b"project", &id.to_bytes()], &moonzip::ID);
    address
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow)]
#[serde(rename_all = "camelCase")]
pub struct Project {
    pub id: Uuid,
    pub owner: StoredPubkey,
    pub token_meta: TokenMeta,
    pub deploy_schema: DeploySchema,
    pub stage: Stage,
    pub created_at: DateTime<TZ>,
}

#[derive(Debug, Serialize, Deserialize, sqlx::Type)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "project_stage")]
pub enum Stage {
    Created,
    Confirmed,
    Prelaunch,
    OnCurve,
    Graduated,
}

#[derive(Debug, Serialize, Deserialize, sqlx::FromRow, sqlx::Type, Clone)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "token_meta")]
pub struct TokenMeta {
    pub name: String,
    pub ticker: String,
    pub description: String,
    pub image: String,
    pub website: String,
    pub twitter: String,
    pub telegram: String,
}

#[derive(Debug, Serialize, Deserialize, Clone, sqlx::Type)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "deploy_schema")]
pub struct DeploySchema {
    pub use_static_pool: bool,
    pub curve_pool: CurveVariant,
}

impl DeploySchema {
    pub fn to_project_schema(&self) -> ProjectSchema {
        ProjectSchema {
            use_static_pool: self.use_static_pool,
            curve_pool: match self.curve_pool {
                CurveVariant::Moonzip => CurvePoolVariant::Moonzip,
                CurveVariant::Pumpfun => CurvePoolVariant::Pumpfun,
            },
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

#[derive(
    Debug, Serialize, Deserialize, sqlx::Type, Clone, From, Into, PartialEq, Eq, PartialOrd, Ord,
)]
#[sqlx(transparent, type_name = "pubkey")]
pub struct StoredPubkey(String);

impl From<Pubkey> for StoredPubkey {
    fn from(value: Pubkey) -> Self {
        Self(value.to_string())
    }
}

impl From<StoredPubkey> for Pubkey {
    fn from(value: StoredPubkey) -> Self {
        Pubkey::try_from(value.0.as_str()).expect("invariant: invalid stored pubkey")
    }
}
