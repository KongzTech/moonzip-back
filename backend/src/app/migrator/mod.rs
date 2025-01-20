use super::{
    instructions::{
        mzip, pumpfun, CurveCreate, InitialPurchase, InstructionsBuilder, TransmuterInitArgs,
    },
    storage::{
        project::{self, project_id, CurveVariant, ProjectId, StoredTokenImage, StoredTokenMeta},
        DBTransaction, StorageClient,
    },
};
use anchor_client::anchor_lang::AccountDeserialize;
use anyhow::bail;
use chrono::DateTime;
use keys_provider::KeysProvider;
use moonzip::{moonzip::StaticPool, project::project_address, PROGRAM_AUTHORITY};
use serde::{Deserialize, Serialize};
use services_common::{solana::pool::SolanaPool, TZ};
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_sdk::{commitment_config::CommitmentConfig, signer::Signer};
use sqlx::query_as;
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, instrument};

pub mod keys_provider;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigratorConfig {
    #[serde(with = "humantime_serde")]
    pub tick_interval: Duration,
    pub keys_provider: keys_provider::Config,
}

pub struct Migrator {
    tools: Tools,
    tx: mpsc::Sender<Diff>,
}

impl Migrator {
    /// Limited by max number of accounts for get_multiple_accounts rpc call
    const PAGE_SIZE: usize = 100;

    pub fn serve(solana_pool: SolanaPool, storage: StorageClient, config: MigratorConfig) {
        let keys_provider = keys_provider::KeysProvider::new(storage.clone(), config.keys_provider);
        let pumpfun_meta_rx = pumpfun::MetaFetcher::new(solana_pool.clone()).serve();
        let moonzip_meta_rx = mzip::MetaFetcher::new(solana_pool.clone()).serve();

        let tools = Tools {
            solana_pool: solana_pool.clone(),
            storage: storage.clone(),
            keys_provider: keys_provider.clone(),
            pumpfun_meta_rx,
            moonzip_meta_rx,
        };

        let (tx, rx) = mpsc::channel(Self::PAGE_SIZE);
        let executor = Executor {
            tools: tools.clone(),
            rx,
        };
        executor.serve();

        let migrator = Migrator { tools, tx };

        tokio::spawn(async move {
            loop {
                if let Err(err) = migrator.tick().await {
                    error!("migration tick failed: {err:#}");
                }

                tokio::time::sleep(config.tick_interval).await;
            }
        });
    }

    async fn tick(&self) -> anyhow::Result<()> {
        let mut after =
            DateTime::<TZ>::from_timestamp(0, 0).expect("invariant: unix epoch timestamp");
        while let Some(new_after) = self.tick_page(after).await? {
            after = new_after;
        }

        Ok(())
    }

    async fn tick_page<'a>(&self, after: DateTime<TZ>) -> anyhow::Result<Option<DateTime<TZ>>> {
        let projects = query_as!(
            project::StoredProject,
            r#"SELECT
                id,
                owner,
                deploy_schema AS "deploy_schema: _",
                stage AS "stage: _",
                static_pool_mint AS "static_pool_mint?: _",
                curve_pool_mint AS "curve_pool_mint?: _",
                created_at
            FROM project WHERE stage != $1 AND created_at > $2 ORDER BY created_at ASC LIMIT $3"#,
            project::Stage::Graduated as _,
            after as _,
            Self::PAGE_SIZE as i64
        )
        .fetch_all(&*self.tools.storage)
        .await?;
        let received_projects = projects.len();
        let last_timemark = projects.last().map(|project| project.created_at);

        let project_keys = projects
            .iter()
            .map(|project| project_address(&project_id(&project.id)))
            .collect::<Vec<_>>();

        let client = self.tools.solana_pool.client().rpc();
        let accounts = client
            .use_single()
            .await
            .get_multiple_accounts_with_config(
                &project_keys,
                RpcAccountInfoConfig {
                    commitment: Some(CommitmentConfig::confirmed()),
                    ..Default::default()
                },
            )
            .await?;
        let iter = projects
            .into_iter()
            .zip(accounts.value.into_iter())
            .filter_map(|(project, account)| {
                let account = account?;
                Some((project, account))
            });
        for (project, account) in iter {
            let project_data = moonzip::project::Project::try_deserialize(&mut &account.data[..])?;
            let diff = Diff {
                stored: project,
                received: project_data,
            };
            self.tx.send(diff).await?;
        }

        if received_projects < Self::PAGE_SIZE {
            Ok(None)
        } else {
            Ok(last_timemark)
        }
    }
}

struct Executor {
    pub tools: Tools,
    pub rx: mpsc::Receiver<Diff>,
}

impl Executor {
    fn serve(self) {
        tokio::spawn(async move {
            if let Err(err) = self.run().await {
                error!("migration executor failed: {err:#}");
            }
        });
    }

    async fn run(mut self) -> anyhow::Result<()> {
        while let Some(diff) = self.rx.recv().await {
            let id = diff.stored.id;
            if let Err(err) = self.migrate_diff(diff).await {
                error!("failed to execute migration for project({:?}): {err:#}", id);
            }
        }

        Ok(())
    }

    #[instrument(skip(self))]
    async fn migrate_diff(&self, diff: Diff) -> anyhow::Result<()> {
        match (&diff.stored.stage, &diff.received.stage) {
            (project::Stage::Created, moonzip::project::ProjectStage::Created) => {
                if diff.stored.deploy_schema.use_static_pool {
                    bail!("invariant: project must have begun straight to static pool");
                }
                let mut worker = Worker {
                    tools: self.tools.clone(),
                    project: &diff.stored,
                    tx: self
                        .lock_project(&diff.stored.id, |stage| stage == project::Stage::Created)
                        .await?,
                };
                worker.init_curve_pool().await?;
            }
            (project::Stage::OnStaticPool, moonzip::project::ProjectStage::StaticPoolClosed) => {
                todo!()
            }
            _ => {}
        }
        Ok(())
    }

    async fn lock_project(
        &self,
        project_id: &ProjectId,
        verify_stage: impl Fn(project::Stage) -> bool,
    ) -> anyhow::Result<DBTransaction<'_>> {
        #[derive(Debug)]
        struct LockResponse {
            stage: project::Stage,
        }

        let mut tx = self.tools.storage.serializable_tx().await?;
        let project = query_as!(
            LockResponse,
            r#"SELECT stage AS "stage: _" FROM project WHERE id = $1 FOR UPDATE"#,
            project_id as _,
        )
        .fetch_one(&mut *tx)
        .await?;
        if !verify_stage(project.stage) {
            bail!(
                "project stage mismatch(actual {:?}): updated by different process",
                project.stage
            );
        }
        Ok(tx)
    }
}

struct Worker<'a> {
    tools: Tools,
    project: &'a project::StoredProject,
    tx: DBTransaction<'a>,
}

impl<'a> Worker<'a> {
    async fn init_curve_pool(&mut self) -> anyhow::Result<()> {
        let metadata_uri = self.deploy_metadata().await?;

        let mut initial_purchase = self
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .cloned()
            .map(TryInto::try_into)
            .transpose()?
            .unwrap_or(0u64);

        if self.project.deploy_schema.use_static_pool {
            let pool = self
                .tools
                .solana_pool
                .client()
                .rpc()
                .use_single()
                .await
                .get_account_data(
                    &self
                        .project
                        .static_pool_mint
                        .as_ref()
                        .ok_or_else(|| {
                            anyhow::anyhow!("invariant: static pool mint is not already stored")
                        })?
                        .to_pubkey(),
                )
                .await?;
            let collected_lamports =
                StaticPool::try_deserialize(&mut &pool[..])?.collected_lamports;
            initial_purchase += collected_lamports;
        }

        let mut ix_builder = InstructionsBuilder {
            solana_pool: self.tools.solana_pool.clone(),
            project: self.project,
        };
        let mut ixs = ix_builder.lock_project()?;

        if self.project.deploy_schema.use_static_pool {
            ixs.append(&mut ix_builder.graduate_static_pool()?);
        }

        let keypair = self.tools.keys_provider.next().await?;

        let token_meta = self.token_meta().await?;
        let curve_create = CurveCreate {
            mint: keypair.pubkey(),
            initial_purchase: InitialPurchase {
                user: PROGRAM_AUTHORITY,
                amount: initial_purchase,
            },
            metadata_uri,
        };

        match self.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => {
                let moonzip_meta = self.tools.moonzip_meta_rx.get().await?;
                ixs.append(&mut ix_builder.init_moonzip_pool(curve_create, &moonzip_meta)?);
            }
            CurveVariant::Pumpfun => {
                let pumpfun_meta = self.tools.pumpfun_meta_rx.get().await?;
                ixs.append(&mut ix_builder.init_pumpfun_pool(
                    curve_create,
                    pumpfun_meta,
                    &token_meta,
                )?);
            }
        };

        // TODO: correctly init transmuters
        ix_builder.add_transmuter_for_moonzip(TransmuterInitArgs {
            from_mint: self.project.static_pool_mint.as_ref().unwrap().to_pubkey(),
            to_mint: keypair.pubkey(),
            donor: PROGRAM_AUTHORITY,
        })?;
        ix_builder.add_transmuter_for_pumpfun(TransmuterInitArgs {
            from_mint: self.project.static_pool_mint.as_ref().unwrap().to_pubkey(),
            to_mint: keypair.pubkey(),
            donor: PROGRAM_AUTHORITY,
        })?;

        ixs.append(&mut ix_builder.unlock_project()?);

        Ok(())
    }

    async fn deploy_metadata(&mut self) -> anyhow::Result<String> {
        match self.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => todo!(),
            CurveVariant::Pumpfun => self.deploy_pumpfun_metadata().await,
        }
    }

    async fn deploy_pumpfun_metadata(&mut self) -> anyhow::Result<String> {
        let meta = self.token_meta().await?;
        let image = self.token_image().await?;

        let client = pumpfun::HttpClient::new();
        let metadata = pumpfun::CreateTokenMetadata {
            name: meta.name,
            symbol: meta.symbol,
            description: meta.description,
            image_content: image.image_content,
            telegram: meta.telegram,
            website: meta.website,
            twitter: meta.twitter,
        };

        let response = client.deploy_metadata(metadata).await?;
        sqlx::query!(
            "UPDATE token_meta SET deployed_url = $1 WHERE project_id = $2",
            response.metadata_uri,
            self.project.id as _,
        )
        .execute(&mut *self.tx)
        .await?;

        Ok(response.metadata_uri)
    }

    async fn token_meta(&mut self) -> anyhow::Result<StoredTokenMeta> {
        let metadata = query_as!(
            StoredTokenMeta,
            "SELECT * FROM token_meta WHERE project_id = $1",
            self.project.id as _,
        )
        .fetch_one(&mut *self.tx)
        .await?;

        Ok(metadata)
    }

    async fn token_image(&mut self) -> anyhow::Result<StoredTokenImage> {
        let image = query_as!(
            StoredTokenImage,
            "SELECT * FROM token_image WHERE project_id = $1",
            self.project.id as _,
        )
        .fetch_one(&mut *self.tx)
        .await?;

        Ok(image)
    }
}

#[derive(Clone)]
struct Tools {
    solana_pool: SolanaPool,
    storage: StorageClient,
    keys_provider: KeysProvider,
    pumpfun_meta_rx: pumpfun::MetaReceiver,
    moonzip_meta_rx: mzip::MetaReceiver,
}

#[derive(Debug, Clone)]
struct Diff {
    stored: project::StoredProject,
    received: moonzip::project::Project,
}
