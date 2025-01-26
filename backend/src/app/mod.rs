use anyhow::Context as _;
use instructions::InstructionsBuilder;
use serde::{Deserialize, Serialize};
use services_common::utils::serialize_tx_bs58;
use solana_sdk::{pubkey::Pubkey, signature::Keypair, signer::Signer, transaction::Transaction};
use storage::{
    project::{DeploySchema, StoredTokenImage},
    StorageClient,
};
use uuid::Uuid;
use validator::Validate;

pub mod instructions;
pub mod keys_loader;
pub mod migrator;
pub mod storage;

pub struct App {
    storage: StorageClient,
    instructions_builder: InstructionsBuilder,
}

impl App {
    pub async fn new(storage: StorageClient, instructions_builder: InstructionsBuilder) -> Self {
        Self {
            storage,
            instructions_builder,
        }
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> anyhow::Result<CreateProjectResponse> {
        request
            .deploy_schema
            .validate()
            .context("validate deploy schema")?;

        let static_pool_keypair = if request.deploy_schema.use_static_pool {
            Some(Keypair::new())
        } else {
            None
        };

        let project = storage::project::StoredProject {
            id: Uuid::new_v4(),
            owner: request.owner.into(),
            deploy_schema: request.deploy_schema.clone(),
            stage: storage::project::Stage::Created,
            static_pool_pubkey: static_pool_keypair
                .as_ref()
                .map(|keypair| keypair.pubkey().into()),
            curve_pool_keypair: None,
            created_at: chrono::Utc::now(),
        };
        let mut builder = self.instructions_builder.for_project(&project).await?;
        let mut ixs = vec![];
        ixs.extend(builder.create_project()?);
        if let Some(keypair) = static_pool_keypair.as_ref() {
            ixs.extend(builder.init_static_pool(keypair)?);
        }

        let transaction = Transaction::new_with_payer(&ixs, Some(&request.owner));

        let image = StoredTokenImage {
            project_id: project.id,
            image_content: request.meta.image_content,
        };

        let mut tx = self.storage.serializable_tx().await?;

        sqlx::query!(
            "INSERT INTO project VALUES ($1, $2, $3, $4, $5, $6, $7)",
            project.id,
            project.owner as _,
            project.deploy_schema as _,
            project.stage as _,
            project.static_pool_pubkey as _,
            project.curve_pool_keypair as _,
            project.created_at
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "INSERT INTO token_meta VALUES ($1, $2, $3, $4, $5, $6, $7)",
            project.id,
            request.meta.name,
            request.meta.symbol,
            request.meta.description,
            request.meta.website,
            request.meta.twitter,
            request.meta.telegram,
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!(
            "INSERT INTO token_image VALUES ($1, $2)",
            project.id,
            image.image_content
        )
        .execute(&mut *tx)
        .await?;

        tx.commit().await?;

        Ok(CreateProjectResponse {
            project_id: project.id,
            transaction,
        })
    }
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CreateProjectRequest {
    pub owner: Pubkey,
    pub meta: CreateTokenMeta,
    pub deploy_schema: DeploySchema,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct CreateTokenMeta {
    pub name: String,
    pub symbol: String,
    pub description: String,

    pub image_content: Vec<u8>,

    pub website: Option<String>,
    pub twitter: Option<String>,
    pub telegram: Option<String>,
}

#[derive(Debug, Serialize, Clone)]
pub struct CreateProjectResponse {
    pub project_id: Uuid,
    #[serde(serialize_with = "serialize_tx_bs58")]
    pub transaction: Transaction,
}
