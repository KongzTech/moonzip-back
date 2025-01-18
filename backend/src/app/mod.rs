use crate::{solana::SolanaPool, utils::serialize_tx_bs58};
use moonzip::project::CreateProjectData;
use serde::{Deserialize, Serialize};
use solana_sdk::{pubkey::Pubkey, transaction::Transaction};
use state::{
    project::{DeploySchema, TokenMeta},
    StorageClient,
};
use uuid::Uuid;

pub mod state;

pub struct App {
    storage: StorageClient,
    solana_pool: SolanaPool,
}

impl App {
    pub async fn new(storage: StorageClient, solana_pool: SolanaPool) -> Self {
        Self {
            storage,
            solana_pool,
        }
    }

    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
    ) -> anyhow::Result<CreateProjectResponse> {
        let project = state::project::Project {
            id: Uuid::new_v4(),
            owner: request.owner.into(),
            token_meta: request.meta,
            deploy_schema: request.deploy_schema.clone(),
            stage: state::project::Stage::Created,
            created_at: chrono::Utc::now(),
        };
        let project_id = state::project::project_id(&project.id);
        let project_address = state::project::project_address(&project_id);

        let client = self.solana_pool.for_authority();
        let program = client.program(moonzip::ID)?;

        let ix = program
            .request()
            .accounts(moonzip::accounts::CreateProjectAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                creator: request.owner,
                project: project_address,
                system_program: solana_sdk::system_program::ID,
            })
            .args(moonzip::instruction::CreateProject {
                data: CreateProjectData {
                    id: project_id,
                    schema: request.deploy_schema.to_project_schema(),
                },
            })
            .instructions()?;

        let transaction = Transaction::new_with_payer(&ix, Some(&request.owner));

        sqlx::query!(
            "INSERT INTO project VALUES ($1, $2, $3, $4, $5, $6)",
            project.id,
            project.owner as _,
            project.token_meta as _,
            project.deploy_schema as _,
            project.stage as _,
            project.created_at
        )
        .execute(&*self.storage)
        .await?;

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
    pub meta: TokenMeta,
    pub deploy_schema: DeploySchema,
}

#[derive(Debug, Serialize, Clone)]
pub struct CreateProjectResponse {
    pub project_id: Uuid,
    #[serde(serialize_with = "serialize_tx_bs58")]
    pub transaction: Transaction,
}
