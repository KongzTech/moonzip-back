use super::{
    instructions::{
        mzip, pumpfun, solana, CurveCreate, InitialPurchase, InstructionsBuilder,
        TransmuterInitArgs,
    },
    storage::{
        project::{self, project_id, CurveVariant, ProjectId, StoredTokenImage, StoredTokenMeta},
        DBTransaction, StorageClient,
    },
};
use crate::solana::SolanaKeys;
use anchor_client::anchor_lang::AccountDeserialize;
use anyhow::bail;
use chrono::DateTime;
use derive_more::derive::Deref;
use moonzip::{moonzip::StaticPool, project::project_address, PROGRAM_AUTHORITY};
use serde::{Deserialize, Serialize};
use services_common::{
    solana::{jito, pool::SolanaPool},
    utils::period_fetch::{DataReceiver, PeriodicFetcher, PeriodicFetcherConfig},
    TZ,
};
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_sdk::{commitment_config::CommitmentConfig, signer::Signer};
use sqlx::{query, query_as};
use std::{ops::DerefMut, sync::Arc, time::Duration};
use tokio::spawn;
use tracing::{error, instrument, warn};
use txs::{TransactionRequest, TxExecutor, TxExecutorConfig};

const DEV_WEBSITE: &str = "https://moon.zip";

pub mod ipfs;
pub mod txs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigratorConfig {
    #[serde(with = "humantime_serde", default = "default_tick_interval")]
    pub tick_interval: Duration,
    pub ipfs: ipfs::IpfsClientConfig,
    pub tx_exec: TxExecutorConfig,
}

pub fn default_tick_interval() -> Duration {
    Duration::from_secs(3)
}

pub struct Migrator {
    tools: Tools,
}

impl Migrator {
    /// Limited by max number of accounts for get_multiple_accounts rpc call
    const PAGE_SIZE: usize = 100;

    pub async fn serve(
        solana_pool: SolanaPool,
        solana_meta: DataReceiver<solana::Meta>,
        solana_keys: SolanaKeys,
        storage: StorageClient,
        instructions_builder: InstructionsBuilder,
        config: MigratorConfig,
    ) -> anyhow::Result<()> {
        let pumpfun_meta_rx = PeriodicFetcher::new(
            pumpfun::MetaFetcher {
                pool: solana_pool.clone(),
            },
            PeriodicFetcherConfig::every_hour(),
        )
        .serve();
        let moonzip_meta_rx = PeriodicFetcher::new(
            mzip::MetaFetcher {
                pool: solana_pool.clone(),
            },
            PeriodicFetcherConfig::every_hour(),
        )
        .serve();
        let jito_meta_rx = PeriodicFetcher::new(
            jito::JitoTipStateFetcher::default(),
            PeriodicFetcherConfig::zero(),
        )
        .serve();

        let ipfs = ipfs::IpfsClient::new(config.ipfs)?;
        ipfs.verify_connection().await?;

        let tools = Tools {
            internal: Arc::new(ToolsInternal {
                solana_pool: solana_pool.clone(),
                storage: storage.clone(),
                pumpfun_meta_rx: pumpfun_meta_rx.clone(),
                moonzip_meta_rx: moonzip_meta_rx.clone(),
                jito_meta_rx: jito_meta_rx.clone(),
                ipfs,
                tx_executor: TxExecutor::new(
                    solana_pool.clone(),
                    solana_meta.clone(),
                    config.tx_exec,
                ),
                solana_keys,
                instructions_builder,
            }),
        };

        let migrator = Migrator { tools };

        tokio::spawn(async move {
            loop {
                if let Err(err) = migrator.tick().await {
                    error!("migration tick failed: {err:#}");
                }

                tokio::time::sleep(config.tick_interval).await;
            }
        });
        Ok(())
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
                static_pool_pubkey AS "static_pool_pubkey?: _",
                curve_pool_keypair AS "curve_pool_keypair?: _",
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

        let client = self.tools.solana_pool.rpc_client();
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
            let executor = DiffExecutor {
                tools: self.tools.clone(),
                diff,
            };
            spawn(async move {
                let id = executor.diff.stored.id;
                if let Err(err) = executor.migrate().await {
                    warn!("failed to execute migration for project({:?}): {err:#}", id);
                }
            });
        }

        if received_projects < Self::PAGE_SIZE {
            Ok(None)
        } else {
            Ok(last_timemark)
        }
    }
}

struct DiffExecutor {
    tools: Tools,
    diff: Diff,
}

impl DiffExecutor {
    #[instrument(skip(self), fields(project_id = %self.diff.stored.id))]
    async fn migrate(self) -> anyhow::Result<()> {
        match (&self.diff.stored.stage, &self.diff.received.stage) {
            // project is created with graduation straight to curve pool
            (project::Stage::Created, moonzip::project::ProjectStage::Created) => {
                if self.diff.stored.deploy_schema.use_static_pool {
                    bail!("invariant: project must have begun straight to static pool");
                }
                let prepare_curve_deploy = PrepareCurveDeploy {
                    tools: self.tools.clone(),
                    lock: self
                        .tools
                        .lock_project_with_stage(&self.diff.stored.id, |stage| {
                            stage == project::Stage::Created
                        })
                        .await?,
                };
                prepare_curve_deploy.prepare().await?;

                let mut second_lock = self
                    .tools
                    .lock_project_with_stage(&self.diff.stored.id, |stage| {
                        stage == project::Stage::Created
                    })
                    .await?;
                let mut deployer = Deployer::new(self.tools.clone(), &mut second_lock);
                deployer.init_curve_pool().await?;
            }
            // we need to migrate static pool to curve pool
            (_, moonzip::project::ProjectStage::StaticPoolClosed) => {
                let mut lock = self.tools.lock_project(&self.diff.stored.id).await?;
                lock.sync_stage(project::Stage::StaticPoolClosed).await?;

                let prepare_curve_deploy = PrepareCurveDeploy {
                    tools: self.tools.clone(),
                    lock,
                };
                prepare_curve_deploy.prepare().await?;

                let mut second_lock = self.tools.lock_project(&self.diff.stored.id).await?;
                let mut deployer = Deployer::new(self.tools.clone(), &mut second_lock);
                deployer.init_curve_pool().await?;
            }
            (_, moonzip::project::ProjectStage::StaticPoolActive) => {
                let mut lock = self.tools.lock_project(&self.diff.stored.id).await?;
                if lock.project.stage != project::Stage::OnStaticPool {
                    lock.sync_stage(project::Stage::OnStaticPool).await?;
                    lock.commit().await?;
                    lock = self.tools.lock_project(&self.diff.stored.id).await?;
                }
                {
                    let mut deployer = Deployer::new(self.tools.clone(), &mut lock);
                    if deployer.should_close_static_pool().await? {
                        deployer.init_curve_pool().await?;
                    }
                }
                lock.commit().await?;
            }
            (_, moonzip::project::ProjectStage::CurvePoolClosed) => {
                let mut lock = self.tools.lock_project(&self.diff.stored.id).await?;
                lock.sync_stage(project::Stage::CurvePoolClosed).await?;
                let deployer = Deployer::new(self.tools.clone(), &mut lock);
                deployer.graduate_to_raydium().await?;
            }
            (_, moonzip::project::ProjectStage::CurvePoolActive) => {
                let mut lock = self.tools.lock_project(&self.diff.stored.id).await?;
                lock.sync_stage(project::Stage::OnCurvePool).await?;
                lock.commit().await?;
            }
            (_, moonzip::project::ProjectStage::Graduated) => {
                let mut lock = self.tools.lock_project(&self.diff.stored.id).await?;
                lock.sync_stage(project::Stage::Graduated).await?;
                lock.commit().await?;
            }
            _ => {
                bail!(
                    "unexpected project migration: {:?} -> {:?}",
                    self.diff.stored.stage,
                    self.diff.received.stage
                );
            }
        }
        Ok(())
    }
}

struct Deployer<'a, 'b> {
    tools: Tools,
    lock: &'a mut ProjectLock<'b>,
    fetched_static_pool: Option<StaticPool>,
}

impl<'a, 'b> Deployer<'a, 'b> {
    fn new(tools: Tools, lock: &'a mut ProjectLock<'b>) -> Self {
        Self {
            tools,
            lock,
            fetched_static_pool: None,
        }
    }

    async fn init_curve_pool(&mut self) -> anyhow::Result<()> {
        let mut initial_purchase = self
            .lock
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .cloned()
            .map(TryInto::try_into)
            .transpose()?
            .unwrap_or(0u64);

        if self.lock.project.deploy_schema.use_static_pool {
            initial_purchase += self.static_pool().await?.collected_lamports;
        }

        let mut ix_builder = self
            .tools
            .instructions_builder
            .for_project(&self.lock.project)
            .await?;
        let mut ixs = ix_builder.lock_project()?;

        if self.lock.project.deploy_schema.use_static_pool {
            ixs.append(&mut ix_builder.graduate_static_pool()?);
        }

        let keypair = self
            .lock
            .project
            .curve_pool_keypair
            .as_ref()
            .map(|keypair| keypair.to_keypair())
            .ok_or_else(|| {
                anyhow::anyhow!("invariant: curve pool secret key is not already stored")
            })?;
        let token_meta = token_meta(&mut self.lock.tx, self.lock.project.id).await?;
        let curve_create = CurveCreate {
            mint: keypair.pubkey(),
            initial_purchase: InitialPurchase {
                user: PROGRAM_AUTHORITY,
                amount: initial_purchase,
            },
            metadata: token_meta,
        };

        match self.lock.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => {
                let moonzip_meta = self.tools.moonzip_meta_rx.clone().get().await?;
                ixs.append(&mut ix_builder.init_moonzip_pool(curve_create, &moonzip_meta)?);
            }
            CurveVariant::Pumpfun => {
                let pumpfun_meta = self.tools.pumpfun_meta_rx.clone().get().await?;
                ixs.append(&mut ix_builder.init_pumpfun_pool(curve_create, pumpfun_meta)?);
            }
        };

        if self.lock.project.deploy_schema.use_static_pool {
            let static_pool_mint = self.lock.project.static_pool_mint().ok_or_else(|| {
                anyhow::anyhow!("invariant: static pool mint is not already stored")
            })?;
            match self.lock.project.deploy_schema.curve_pool {
                CurveVariant::Moonzip => {
                    ix_builder.add_transmuter_for_moonzip(TransmuterInitArgs {
                        from_mint: static_pool_mint,
                        to_mint: keypair.pubkey(),
                        donor: PROGRAM_AUTHORITY,
                    })?;
                }
                CurveVariant::Pumpfun => {
                    ix_builder.add_transmuter_for_pumpfun(TransmuterInitArgs {
                        from_mint: static_pool_mint,
                        to_mint: keypair.pubkey(),
                        donor: PROGRAM_AUTHORITY,
                    })?;
                }
            }
        };

        ixs.append(&mut ix_builder.unlock_project()?);

        self.tools
            .tx_executor
            .execute_single(TransactionRequest {
                instructions: ixs,
                signers: vec![self.tools.solana_keys.authority_keypair().to_keypair()],
                payer: self.tools.solana_keys.authority_keypair().to_keypair(),
            })
            .await?;

        Ok(())
    }

    async fn should_close_static_pool(&mut self) -> anyhow::Result<bool> {
        let pool = self.static_pool().await?;

        Ok(pool
            .config
            .close_conditions
            .should_be_closed(pool.collected_lamports, TZ::now().timestamp() as u64))
    }

    async fn graduate_to_raydium(&self) -> anyhow::Result<()> {
        let ix_builder = self
            .tools
            .instructions_builder
            .for_project(&self.lock.project)
            .await?;
        let mut first_tx = ix_builder.graduate_curve_pool()?;
        let (openbook_market, mut openbook_market_ix) = ix_builder.prepare_openbook_market()?;
        first_tx.append(&mut openbook_market_ix);

        let jito_meta = self.tools.jito_meta_rx.clone().get().await?;
        first_tx.push(jito_meta.tip_ix(&PROGRAM_AUTHORITY));

        let curve_config = self
            .tools
            .moonzip_meta_rx
            .clone()
            .get()
            .await?
            .global_account
            .config
            .curve;
        let tokens_amount =
            curve_config.total_token_supply - curve_config.initial_real_token_reserves;

        let second_tx = ix_builder.deploy_to_raydium(
            &openbook_market,
            tokens_amount,
            self.tools.instructions_builder.config.rayidum_liquidity,
        )?;

        let signer = self.tools.solana_keys.authority_keypair().to_keypair();

        self.tools
            .tx_executor
            .execute_batch(vec![
                TransactionRequest {
                    instructions: first_tx,
                    signers: vec![signer.insecure_clone()],
                    payer: signer.insecure_clone(),
                },
                TransactionRequest {
                    instructions: second_tx,
                    signers: vec![signer.insecure_clone()],
                    payer: signer.insecure_clone(),
                },
            ])
            .await?;

        Ok(())
    }

    async fn static_pool(&mut self) -> anyhow::Result<&StaticPool> {
        {
            if let Some(pool) = self.fetched_static_pool.take() {
                self.fetched_static_pool = Some(pool);
                return Ok(self.fetched_static_pool.as_ref().unwrap());
            }
        }

        let pool = self
            .tools
            .solana_pool
            .rpc_client()
            .use_single()
            .await
            .get_account_data(&self.lock.project.static_pool_mint().ok_or_else(|| {
                anyhow::anyhow!("invariant: static pool mint is not already stored")
            })?)
            .await?;
        let pool = StaticPool::try_deserialize(&mut &pool[..])?;
        self.fetched_static_pool = Some(pool);
        Ok(self.fetched_static_pool.as_ref().unwrap())
    }
}

struct PrepareCurveDeploy<'a> {
    tools: Tools,
    lock: ProjectLock<'a>,
}

impl<'a> PrepareCurveDeploy<'a> {
    async fn prepare(mut self) -> anyhow::Result<()> {
        query!(
            r#"call assign_project_keypair($1)"#,
            self.lock.project.id as _,
        )
        .execute(&mut *self.lock.tx)
        .await?;

        let mut manager = MetadataManager {
            tools: self.tools.clone(),
            project_id: self.lock.project.id,
            tx: &mut self.lock.tx,
        };

        manager
            .deploy_metadata(self.lock.project.deploy_schema.curve_pool)
            .await?;

        self.lock.commit().await?;

        Ok(())
    }
}

struct MetadataManager<'a, 'b> {
    tools: Tools,
    project_id: ProjectId,
    tx: &'a mut DBTransaction<'b>,
}

impl<'a, 'b> MetadataManager<'a, 'b> {
    async fn deploy_metadata(&mut self, curve_variant: CurveVariant) -> anyhow::Result<String> {
        let meta = token_meta(self.tx, self.project_id).await?;
        if let Some(deployed_url) = meta.deployed_url {
            return Ok(deployed_url);
        }

        let image = token_image(self.tx, self.project_id).await?;

        let metadata_uri = match curve_variant {
            CurveVariant::Moonzip => self.deploy_moonzip_metadata(meta, image).await?,
            CurveVariant::Pumpfun => self.deploy_pumpfun_metadata(meta, image).await?,
        };

        sqlx::query!(
            "UPDATE token_meta SET deployed_url = $1 WHERE project_id = $2",
            metadata_uri,
            self.project_id as _,
        )
        .execute(self.tx.deref_mut())
        .await?;

        Ok(metadata_uri)
    }

    async fn deploy_pumpfun_metadata(
        &mut self,
        meta: StoredTokenMeta,
        image: StoredTokenImage,
    ) -> anyhow::Result<String> {
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

        Ok(response.metadata_uri)
    }

    async fn deploy_moonzip_metadata(
        &mut self,
        meta: StoredTokenMeta,
        image: StoredTokenImage,
    ) -> anyhow::Result<String> {
        let image_url = self
            .tools
            .ipfs
            .upload_image(image.image_content, &meta.name)
            .await?;
        let token_name = meta.name.clone();

        let metadata = OffchainMetadata {
            name: meta.name,
            symbol: meta.symbol,
            description: meta.description,
            image: image_url,
            show_name: true,
            created_on: DEV_WEBSITE.to_string(),
            telegram: meta.telegram,
            website: meta.website,
            twitter: meta.twitter,
        };

        let meta_url = self.tools.ipfs.upload_json(&metadata, &token_name).await?;
        Ok(meta_url)
    }
}

#[derive(Clone, Deref)]
struct Tools {
    internal: Arc<ToolsInternal>,
}

struct ToolsInternal {
    solana_pool: SolanaPool,
    solana_keys: SolanaKeys,

    storage: StorageClient,
    pumpfun_meta_rx: DataReceiver<pumpfun::Meta>,
    moonzip_meta_rx: DataReceiver<mzip::Meta>,
    jito_meta_rx: DataReceiver<jito::TipState>,

    ipfs: ipfs::IpfsClient,
    tx_executor: TxExecutor,
    instructions_builder: InstructionsBuilder,
}

impl Tools {
    async fn lock_project<'a>(&self, project_id: &ProjectId) -> anyhow::Result<ProjectLock<'_>> {
        let mut tx = self.storage.serializable_tx().await?;

        let project = query_as!(
            project::StoredProject,
            r#"SELECT
                id,
                owner,
                deploy_schema AS "deploy_schema: _",
                stage AS "stage: _",
                static_pool_pubkey AS "static_pool_pubkey?: _",
                curve_pool_keypair AS "curve_pool_keypair?: _",
                created_at
            FROM project WHERE id = $1 FOR UPDATE NOWAIT"#,
            project_id as _,
        )
        .fetch_one(&mut *tx)
        .await?;

        Ok(ProjectLock { tx, project })
    }

    async fn lock_project_with_stage<'a>(
        &'a self,
        project_id: &ProjectId,
        verify_stage: impl Fn(project::Stage) -> bool,
    ) -> anyhow::Result<ProjectLock<'a>> {
        let lock = self.lock_project(project_id).await?;
        if !verify_stage(lock.project.stage) {
            bail!(
                "project stage mismatch(actual {:?}): updated by different process",
                lock.project.stage
            );
        }
        Ok(lock)
    }
}

struct ProjectLock<'a> {
    tx: DBTransaction<'a>,
    project: project::StoredProject,
}

async fn set_project_stage(
    tx: &mut DBTransaction<'_>,
    project_id: ProjectId,
    stage: project::Stage,
) -> anyhow::Result<()> {
    query!(
        r#"UPDATE project SET stage = $1 WHERE id = $2"#,
        stage as _,
        project_id as _,
    )
    .execute(tx.deref_mut())
    .await?;
    Ok(())
}

impl<'a> ProjectLock<'a> {
    async fn sync_stage(&mut self, stage: project::Stage) -> anyhow::Result<()> {
        set_project_stage(&mut self.tx, self.project.id, stage).await?;
        self.project.stage = stage;
        Ok(())
    }

    async fn commit(self) -> anyhow::Result<()> {
        self.tx.commit().await?;
        Ok(())
    }
}

async fn token_meta(
    tx: &mut DBTransaction<'_>,
    project_id: ProjectId,
) -> anyhow::Result<StoredTokenMeta> {
    let metadata = query_as!(
        StoredTokenMeta,
        "SELECT * FROM token_meta WHERE project_id = $1",
        project_id as _,
    )
    .fetch_one(tx.deref_mut())
    .await?;

    Ok(metadata)
}

async fn token_image(
    tx: &mut DBTransaction<'_>,
    project_id: ProjectId,
) -> anyhow::Result<StoredTokenImage> {
    let image = query_as!(
        StoredTokenImage,
        "SELECT * FROM token_image WHERE project_id = $1",
        project_id as _,
    )
    .fetch_one(tx.deref_mut())
    .await?;

    Ok(image)
}

#[derive(Debug, Clone)]
struct Diff {
    stored: project::StoredProject,
    received: moonzip::project::Project,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(rename_all = "camelCase")]
struct OffchainMetadata {
    name: String,
    symbol: String,
    description: String,
    image: String,
    show_name: bool,
    created_on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    telegram: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    website: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    twitter: Option<String>,
}
