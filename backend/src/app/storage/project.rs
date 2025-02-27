use crate::app::exposed::{DevLockPeriod, DevPurchase};

use super::{
    misc::{Balance, StoredKeypair, StoredPubkey},
    DB,
};
use bytes::Bytes;
use chrono::DateTime;
use const_format::concatcp;
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

pub fn from_chain_project_id(id: moonzip::project::ProjectId) -> ProjectId {
    Uuid::from_u128_le(id.0)
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
    pub dev_lock_keypair: Option<StoredKeypair>,
    pub created_at: DateTime<TZ>,
}

impl StoredProject {
    pub fn apply_from_chain(&mut self, project: moonzip::project::Project) -> bool {
        let stage = Stage::from_chain(project.stage);
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

#[derive(Debug, sqlx::Type, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
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

impl Stage {
    pub fn from_chain(onchain: moonzip::project::ProjectStage) -> Self {
        match onchain {
            ProjectStage::Created => Stage::Confirmed,
            ProjectStage::StaticPoolActive => Stage::OnStaticPool,
            ProjectStage::StaticPoolClosed => Stage::StaticPoolClosed,
            ProjectStage::CurvePoolActive => Stage::OnCurvePool,
            ProjectStage::CurvePoolClosed => Stage::CurvePoolClosed,
            ProjectStage::Graduated => Stage::Graduated,
        }
    }
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
#[sqlx(type_name = "dev_purchase")]
pub struct StoredDevPurchase {
    pub amount: Balance,
    pub lock_period: i64,
}

impl From<DevPurchase> for StoredDevPurchase {
    fn from(purchase: DevPurchase) -> Self {
        Self {
            amount: purchase.value.into(),
            lock_period: purchase.lock.as_secs() as i64,
        }
    }
}

impl TryFrom<StoredDevPurchase> for DevPurchase {
    type Error = anyhow::Error;

    fn try_from(purchase: StoredDevPurchase) -> Result<Self, Self::Error> {
        Ok(Self {
            value: purchase.amount.try_into()?,
            lock: DevLockPeriod::from_secs(purchase.lock_period as u64),
        })
    }
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "deploy_schema")]
pub struct StoredDeploySchema {
    pub static_pool: Option<StoredStaticPoolConfig>,
    pub curve_pool: CurveVariant,
    pub dev_purchase: Option<StoredDevPurchase>,
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

#[derive(
    Debug, Serialize, Deserialize, Clone, Copy, sqlx::Type, ToSchema, PartialEq, PartialOrd,
)]
#[serde(rename_all = "camelCase")]
#[sqlx(type_name = "curve_variant")]
pub enum CurveVariant {
    Moonzip,
    Pumpfun,
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "static_pool_state")]
pub struct StaticPoolState {
    pub collected_lamports: Balance,
}

#[derive(Debug, Clone, sqlx::Type)]
#[sqlx(type_name = "pumpfun_curve_state")]
pub struct PumpfunCurveState {
    pub virtual_sol_reserves: Balance,
    pub virtual_token_reserves: Balance,
}

#[derive(sqlx::FromRow, Clone)]
pub struct FullProjectState {
    #[sqlx(flatten)]
    pub project: StoredProject,
    pub static_pool_state: Option<StaticPoolState>,
    pub pumpfun_curve_state: Option<PumpfunCurveState>,
}

impl FullProjectState {
    pub const QUERY_BODY: &str = r#"
            SELECT
                project.id AS id,
                project.owner AS owner,
                project.deploy_schema AS deploy_schema,
                project.stage AS stage,
                project.static_pool_pubkey AS static_pool_pubkey,
                project.curve_pool_keypair AS curve_pool_keypair,
                project.dev_lock_keypair AS dev_lock_keypair,
                project.created_at AS created_at,
                static_pool_chain_state.state AS static_pool_state,
                pumpfun_chain_state.state AS pumpfun_curve_state
            FROM project
            LEFT JOIN static_pool_chain_state ON project.id = static_pool_chain_state.project_id
            LEFT JOIN pumpfun_chain_state ON pumpfun_chain_state.mint = kp_to_pubkey(project.curve_pool_keypair)
    "#;

    pub fn only_project(project: StoredProject) -> Self {
        Self {
            project,
            static_pool_state: None,
            pumpfun_curve_state: None,
        }
    }

    pub async fn query<'c, E: sqlx::Executor<'c, Database = DB>>(
        executor: E,
        project_id: &ProjectId,
    ) -> anyhow::Result<Self> {
        Ok(sqlx::query_as(concatcp!(
            FullProjectState::QUERY_BODY,
            "WHERE project.id = $1"
        ))
        .bind(project_id)
        .fetch_one(executor)
        .await?)
    }
}

impl FullProjectState {
    pub fn should_close_static_pool(&self) -> bool {
        let pool_schema = self
            .project
            .deploy_schema
            .static_pool
            .as_ref()
            .expect("no static pool in project deploy schema");

        let current_ts = TZ::now().timestamp();

        let mut is_closed = false;
        is_closed = is_closed || (current_ts >= pool_schema.launch_ts);
        is_closed
    }
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
