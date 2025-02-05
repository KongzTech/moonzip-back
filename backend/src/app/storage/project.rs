use super::misc::{Balance, StoredKeypair, StoredPubkey};
use bytes::Bytes;
use chrono::DateTime;
use futures_util::stream::BoxStream;
use moonzip::project::{CurvePoolVariant, ProjectSchema, ProjectStage};
use serde::{Deserialize, Serialize};
use services_common::{utils::SyncStream, TZ};
use solana_sdk::{pubkey::Pubkey, signer::Signer};
use sqlx::types::Uuid;
use utoipa::ToSchema;

pub fn project_id(id: &Uuid) -> moonzip::project::ProjectId {
    moonzip::project::ProjectId::from(id.to_u128_le())
}

pub type ProjectId = Uuid;

#[derive(Debug, sqlx::FromRow, Clone)]
pub struct StoredProject {
    pub id: ProjectId,
    pub owner: StoredPubkey,
    pub deploy_schema: StoredDeploySchema,
    pub stage: Stage,
    pub static_pool_pubkey: Option<StoredPubkey>,
    pub curve_pool_keypair: Option<StoredKeypair>,
    pub created_at: DateTime<TZ>,
}

impl StoredProject {
    pub fn apply_from_chain(&mut self, project: moonzip::project::Project) -> bool {
        let stage = match project.stage {
            ProjectStage::Created => Stage::Confirmed,
            ProjectStage::StaticPoolActive => Stage::OnStaticPool,
            ProjectStage::StaticPoolClosed => Stage::StaticPoolClosed,
            ProjectStage::CurvePoolActive => Stage::OnCurvePool,
            ProjectStage::CurvePoolClosed => Stage::CurvePoolClosed,
            ProjectStage::Graduated => Stage::Graduated,
        };
        let changed = self.stage != stage;
        self.stage = stage;
        changed
    }

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
    Confirmed,
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

pub struct ImageStream<'a>(pub BoxStream<'a, Result<Bytes, sqlx::Error>>);

impl<'a> From<ImageStream<'a>> for reqwest::Body {
    fn from(stream: ImageStream<'a>) -> Self {
        // TODO: avoid unsafe cast, maybe some lifetimes expansion.
        // It's to use with cautious, it's just because reqwest::Body doesn't respect lifetimes.
        let stream: BoxStream<'a, Result<Bytes, sqlx::Error>> = stream.0;
        let stream: BoxStream<'static, Result<Bytes, sqlx::Error>> =
            unsafe { std::mem::transmute(stream) };
        reqwest::Body::wrap_stream(SyncStream::new(stream))
    }
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "static_pool_config")]
pub struct StoredStaticPoolConfig {
    pub launch_ts: i64,
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "deploy_schema")]
pub struct StoredDeploySchema {
    pub static_pool: Option<StoredStaticPoolConfig>,
    pub curve_pool: CurveVariant,
    pub dev_purchase: Option<Balance>,
}

impl StoredDeploySchema {
    pub fn to_project_schema(&self) -> ProjectSchema {
        ProjectSchema {
            use_static_pool: self.static_pool.is_some(),
            curve_pool: match self.curve_pool {
                CurveVariant::Moonzip => CurvePoolVariant::Moonzip,
                CurveVariant::Pumpfun => CurvePoolVariant::Pumpfun,
            },
            dev_purchase: None,
        }
    }
}

#[derive(Debug, Serialize, Deserialize, Clone, Copy, sqlx::Type, ToSchema)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "curve_variant")]
pub enum CurveVariant {
    Moonzip,
    Pumpfun,
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use std::path::Path;

    impl<'a> ImageStream<'a> {
        pub fn from_file(path: &Path) -> anyhow::Result<Self> {
            let image = std::fs::read(path)?;
            Ok(Self(Box::pin(futures_util::stream::once(async move {
                sqlx::Result::Ok(Bytes::from(image))
            }))))
        }
    }
}
