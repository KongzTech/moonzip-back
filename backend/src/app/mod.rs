use crate::solana::SolanaKeys;
use anyhow::bail;
use exposed::{
    BuyRequest, BuyResponse, CreateProjectRequest, CreateProjectResponse, CreateProjectStreamData,
    GetProjectRequest, GetProjectResponse, PublicProject, SellRequest, SellResponse,
    StoredProjectInfo,
};
use instructions::{
    mpl::{SampleMetadata, SAMPLE_MPL_URI},
    InstructionsBuilder,
};
use services_common::utils::period_fetch::DataReceiver;
use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
use sqlx::query_as;
use std::{pin::pin, time::Duration};
use storage::{project::StoredProject, StorageClient};
use tokio::io::AsyncRead;
use tracing::debug;
use uuid::Uuid;

pub mod exposed;
pub mod instructions;
pub mod keys_loader;
pub mod migrator;
pub mod storage;

pub struct App {
    pub storage: StorageClient,
    pub instructions_builder: InstructionsBuilder,
    pub keys: SolanaKeys,
    pub solana_meta: DataReceiver<instructions::solana::Meta>,
}

impl App {
    pub async fn create_project(
        &self,
        request: CreateProjectRequest,
        streams: CreateProjectStreamData<impl AsyncRead>,
    ) -> anyhow::Result<CreateProjectResponse> {
        if let Some(static_pool) = &request.deploy_schema.static_pool {
            if !self
                .instructions_builder
                .config
                .allowed_launch_periods
                .contains(&Duration::from_secs(static_pool.launch_period))
            {
                bail!("Invalid launch period: {}", static_pool.launch_period);
            }
        };
        let deploy_schema = request.deploy_schema.try_to_stored()?;

        let static_pool_keypair = if deploy_schema.static_pool.is_some() {
            Some(Keypair::new())
        } else {
            None
        };

        let project = storage::project::StoredProject {
            id: Uuid::new_v4(),
            owner: request.owner.into(),
            deploy_schema: deploy_schema.clone(),
            stage: storage::project::Stage::Created,
            static_pool_pubkey: static_pool_keypair
                .as_ref()
                .map(|keypair| keypair.pubkey().into()),
            curve_pool_keypair: None,
            created_at: chrono::Utc::now(),
        };
        let mut builder = self.instructions_builder.for_project(&project)?;
        let mut ixs = vec![];
        ixs.extend(builder.create_project(SampleMetadata {
            name: &request.meta.name,
            symbol: &request.meta.symbol,
            uri: SAMPLE_MPL_URI,
        })?);
        if let Some(keypair) = static_pool_keypair.as_ref() {
            ixs.extend(builder.init_static_pool(keypair)?);
        }

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

        let mut copy_in = tx
            .copy_in_raw(
                "COPY token_image (project_id, image_content) FROM STDIN WITH (FORMAT text)",
            )
            .await?;
        copy_in.send(project.id.to_string().as_bytes()).await?;
        copy_in.send(b"\t".as_slice()).await?;
        copy_in.read_from(pin!(streams.image_content)).await?;
        copy_in.finish().await?;

        tx.commit().await?;

        let recent_blockhash = self.solana_meta.clone().get()?.recent_blockhash;
        let mut transaction = Transaction::new_with_payer(&ixs, Some(&request.owner));
        let authority = self.keys.authority_keypair().to_keypair();
        let mut signers = vec![&authority];
        if let Some(keypair) = static_pool_keypair.as_ref() {
            signers.push(keypair);
        }
        transaction.partial_sign(&signers, recent_blockhash);
        Ok(CreateProjectResponse {
            project_id: project.id,
            transaction,
        })
    }

    pub async fn buy(&self, request: BuyRequest) -> anyhow::Result<BuyResponse> {
        let project = self.get_full_project(request.project_id).await?;

        let builder = self.instructions_builder.for_project(&project)?;
        let ixs = builder.buy(request.user, request.tokens, request.max_sol_cost)?;
        let mut tx = Transaction::new_with_payer(&ixs, Some(&request.user));
        let recent_blockhash = self.solana_meta.clone().get()?.recent_blockhash;
        tx.partial_sign(&[&self.keys.authority_keypair()], recent_blockhash);

        Ok(BuyResponse { transaction: tx })
    }

    pub async fn sell(&self, request: SellRequest) -> anyhow::Result<SellResponse> {
        let project = self.get_full_project(request.project_id).await?;

        let builder = self.instructions_builder.for_project(&project)?;
        let ixs = builder.sell(request.user, request.tokens, request.min_sol_output)?;
        let mut tx = Transaction::new_with_payer(&ixs, Some(&request.user));
        let recent_blockhash = self.solana_meta.clone().get()?.recent_blockhash;
        tx.partial_sign(&[&self.keys.authority_keypair()], recent_blockhash);
        Ok(SellResponse { transaction: tx })
    }

    pub async fn get_project(
        &self,
        request: GetProjectRequest,
    ) -> anyhow::Result<GetProjectResponse> {
        let stored_project = query_as!(
            StoredProjectInfo,
            r#"SELECT
                project.id,
                project.owner AS "owner: _",
                token_meta.name,
                token_meta.description,
                project.stage AS "stage: _",
                project.static_pool_pubkey AS "static_pool_pubkey?: _",
                project.curve_pool_keypair AS "curve_pool_keypair?: _",
                project.created_at AS "created_at: _"
            FROM project, token_meta WHERE project.id = $1 AND token_meta.project_id = $1"#,
            request.project_id as _,
        )
        .fetch_one(&self.storage.pool)
        .await?;

        let project = match PublicProject::try_from(stored_project) {
            Ok(project) => project,
            Err(err) => {
                debug!(
                    "Project {} would not be exposed: {}",
                    request.project_id, err
                );
                return Ok(GetProjectResponse { project: None });
            }
        };

        Ok(GetProjectResponse {
            project: Some(project),
        })
    }

    async fn get_full_project(&self, project_id: Uuid) -> anyhow::Result<StoredProject> {
        let project = query_as!(
            StoredProject,
            r#"SELECT
                id,
                owner,
                deploy_schema AS "deploy_schema: _",
                stage AS "stage: _",
                static_pool_pubkey AS "static_pool_pubkey?: _",
                curve_pool_keypair AS "curve_pool_keypair?: _",
                created_at
            FROM project WHERE id = $1"#,
            project_id as _,
        )
        .fetch_one(&self.storage.pool)
        .await?;
        Ok(project)
    }
}
