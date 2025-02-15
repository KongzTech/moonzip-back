use super::{
    instructions::{mzip, pumpfun, CurveCreate, InitialPurchase, InstructionsBuilder},
    storage::{
        project::{self, project_id, CurveVariant, ImageStream, ProjectId, StoredTokenMeta},
        DBTransaction, StorageClient, DB,
    },
};
use crate::solana::SolanaKeys;
use anchor_client::anchor_lang::AccountDeserialize;
use anyhow::{bail, Context as _};
use chrono::DateTime;
use derive_more::derive::Deref;
use moonzip::{
    moonzip::{static_pool_address, StaticPool},
    project::project_address,
    PROGRAM_AUTHORITY,
};
use rust_decimal::prelude::Zero;
use serde::{Deserialize, Serialize};
use services_common::{
    solana::{jito, pool::SolanaPool},
    utils::period_fetch::{DataReceiver, PeriodicFetcher, PeriodicFetcherConfig},
    TZ,
};
use solana_client::rpc_config::RpcAccountInfoConfig;
use solana_sdk::{commitment_config::CommitmentConfig, signer::Signer};
use sqlx::{query, query_as, Executor};
use std::{
    ops::{Deref, DerefMut},
    sync::Arc,
    time::Duration,
};
use tokio::{spawn, task::JoinHandle};
use tracing::{debug, error, instrument, warn};
use txs::{TransactionRequest, TxExecutor, TxExecutorConfig};

const DEV_WEBSITE: &str = "https://moon.zip";

pub mod ipfs;
pub mod txs;

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MigratorConfig {
    #[serde(with = "humantime_serde", default = "default_tick_interval")]
    pub tick_interval: Duration,
    pub mzip_ipfs: ipfs::moonzip::IpfsClientConfig,
    pub pumpfun_ipfs: ipfs::pumpfun::PumpfunIpfsClientConfig,
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
        solana_keys: SolanaKeys,
        storage: StorageClient,
        instructions_builder: InstructionsBuilder,
        config: MigratorConfig,
    ) -> anyhow::Result<JoinHandle<()>> {
        let jito_meta_rx = PeriodicFetcher::new(
            jito::JitoTipStateFetcher::default(),
            PeriodicFetcherConfig::zero(),
        )
        .serve();

        let mzip_ipfs = ipfs::moonzip::IpfsClient::new(config.mzip_ipfs)?;
        mzip_ipfs.verify_connection().await?;

        let pumpfun_ipfs = ipfs::pumpfun::PumpfunIpfsClient::new(config.pumpfun_ipfs);

        let tools = Tools {
            internal: Arc::new(ToolsInternal {
                solana_pool: solana_pool.clone(),
                storage: storage.clone(),
                pumpfun_meta_rx: instructions_builder.pump_meta.clone(),
                moonzip_meta_rx: instructions_builder.mzip_meta.clone(),
                jito_meta_rx: jito_meta_rx.clone(),
                mzip_ipfs,
                pumpfun_ipfs,
                tx_executor: TxExecutor::new(
                    solana_pool.clone(),
                    instructions_builder.solana_meta.clone(),
                    config.tx_exec,
                ),
                solana_keys,
                instructions_builder,
            }),
        };

        let migrator = Migrator { tools };

        Ok(tokio::spawn(async move {
            loop {
                if let Err(err) = migrator.tick().await {
                    error!("migration tick failed: {err:#}");
                }

                tokio::time::sleep(config.tick_interval).await;
            }
        }))
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
                dev_lock_keypair AS "dev_lock_keypair?: _",
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
        for (mut project, account) in iter {
            let previous_stage = project.stage;
            let project_data = moonzip::project::Project::try_deserialize(&mut &account.data[..])?;
            let has_changed = project.apply_from_chain(project_data);
            if has_changed {
                set_project_stage(self.tools.storage.deref(), project.id, project.stage).await?;
                tracing::debug!(
                    "synced project({}) stage {:?} -> {:?}",
                    project.id,
                    previous_stage,
                    project.stage
                )
            }
            if !migrator_target(&project) {
                debug!(
                    "skipping project({:?}) migration - not a target",
                    project.id
                );
                continue;
            }

            let executor = ProjectMigrationExecutor {
                tools: self.tools.clone(),
                project,
            };
            spawn(async move {
                let id = executor.project.id;
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

fn migrator_target(project: &project::StoredProject) -> bool {
    matches!(
        project.stage,
        project::Stage::Confirmed
            | project::Stage::OnStaticPool
            | project::Stage::StaticPoolClosed
            | project::Stage::CurvePoolClosed
    )
}

struct ProjectMigrationExecutor {
    tools: Tools,
    project: project::StoredProject,
}

impl ProjectMigrationExecutor {
    #[instrument(skip(self), fields(project_id = %self.project.id))]
    async fn migrate(self) -> anyhow::Result<()> {
        match self.project.stage {
            // project is created with graduation straight to curve pool
            project::Stage::Confirmed => {
                if self.project.deploy_schema.static_pool.is_some() {
                    bail!("invariant: project must have begun straight to static pool");
                }
                self.deploy_curve(ChainHelper::new(&self.tools.solana_pool, &self.project))
                    .await?;
            }
            // we need to migrate static pool to curve pool
            project::Stage::StaticPoolClosed => {
                self.deploy_curve(ChainHelper::new(&self.tools.solana_pool, &self.project))
                    .await?;
            }
            project::Stage::OnStaticPool => {
                let mut chain_helper = ChainHelper::new(&self.tools.solana_pool, &self.project);
                if chain_helper.should_close_static_pool().await? {
                    self.deploy_curve(chain_helper).await?;
                }
            }
            project::Stage::CurvePoolClosed => {
                let mut lock = self
                    .tools
                    .lock_project_with_stage(&self.project.id, |stage| {
                        stage == project::Stage::CurvePoolClosed
                    })
                    .await?;
                let chain_helper = ChainHelper::new(&self.tools.solana_pool, &self.project);
                let deployer = Deployer::new(self.tools.clone(), &mut lock, chain_helper);
                deployer.graduate_to_raydium().await?;
            }
            _ => {
                bail!("invariant: other stage must not propagate to the migrator");
            }
        }
        Ok(())
    }

    async fn deploy_curve<'a>(&self, chain_helper: ChainHelper<'a>) -> anyhow::Result<()> {
        let verify_stage = |stage: project::Stage| {
            stage == project::Stage::StaticPoolClosed
                || stage == project::Stage::Confirmed
                || stage == project::Stage::OnStaticPool
        };

        let mut lock = self
            .tools
            .lock_project_with_stage(&self.project.id, verify_stage)
            .await?;

        // Assign keypair and deploy project metadata if needed
        if lock.project.curve_pool_mint().is_none() {
            let prepare_curve_deploy = PrepareCurveDeploy {
                tools: self.tools.clone(),
                lock,
            };
            prepare_curve_deploy.prepare().await?;
            lock = self
                .tools
                .lock_project_with_stage(&self.project.id, verify_stage)
                .await?;
        }

        let mut deployer = Deployer::new(self.tools.clone(), &mut lock, chain_helper);
        deployer.init_curve_pool().await?;
        Ok(())
    }
}

struct Deployer<'a, 'b> {
    tools: Tools,
    lock: &'a mut ProjectLock<'b>,
    chain_helper: ChainHelper<'a>,
}

impl<'a, 'b> Deployer<'a, 'b> {
    fn new(tools: Tools, lock: &'a mut ProjectLock<'b>, chain_helper: ChainHelper<'a>) -> Self {
        Self {
            tools,
            lock,
            chain_helper,
        }
    }

    async fn init_curve_pool(&mut self) -> anyhow::Result<()> {
        let dev_purchase = self
            .lock
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .cloned()
            .map(|purchase| purchase.amount.try_into())
            .transpose()?;

        let post_dev_purchase = if self.lock.project.deploy_schema.static_pool.is_some() {
            let collected = self.chain_helper.static_pool().await?.collected_lamports;
            if collected == 0 {
                None
            } else {
                Some(collected)
            }
        } else {
            None
        };

        let mut ix_builder = self
            .tools
            .instructions_builder
            .for_project(&self.lock.project)?;
        let mut first_tx = ix_builder.lock_project()?;

        if self.lock.project.deploy_schema.static_pool.is_some() {
            first_tx.append(&mut ix_builder.graduate_static_pool()?);
        }

        let curve_mint_keypair = self
            .lock
            .project
            .curve_pool_keypair
            .as_ref()
            .map(|keypair| keypair.to_keypair())
            .ok_or_else(|| {
                anyhow::anyhow!("invariant: curve pool secret key is not already stored")
            })?;
        let dev_lock_keypair = self
            .lock
            .project
            .dev_lock_keypair
            .as_ref()
            .map(|keypair| keypair.to_keypair());
        let should_lock = self
            .lock
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .map(|purchase| !purchase.lock_period.is_zero())
            .unwrap_or(false);

        let token_meta = token_meta(&mut self.lock.tx, self.lock.project.id).await?;
        let curve_create = CurveCreate {
            mint: curve_mint_keypair.pubkey(),
            dev_purchase: dev_purchase.map(|sols| InitialPurchase {
                user: PROGRAM_AUTHORITY,
                sols,
            }),
            post_dev_purchase: post_dev_purchase.map(|sols| InitialPurchase {
                user: PROGRAM_AUTHORITY,
                sols,
            }),
            metadata: token_meta,
        };

        match self.lock.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => {
                first_tx.append(&mut ix_builder.init_moonzip_pool(curve_create)?);
            }
            CurveVariant::Pumpfun => {
                let pumpfun_meta = self.tools.pumpfun_meta_rx.clone().get()?;
                first_tx.append(&mut ix_builder.init_pumpfun_pool(curve_create, pumpfun_meta)?);
            }
        };

        // No need to lock, deliver tokens straight to the user.
        if dev_purchase.is_some() && !should_lock {
            first_tx.append(&mut ix_builder.deliver_dev_tokens()?);
            first_tx.append(&mut ix_builder.init_transmuter()?);
        }

        first_tx.append(&mut ix_builder.unlock_project()?);
        let mut txs = vec![TransactionRequest {
            instructions: first_tx,
            signers: vec![
                self.tools.solana_keys.authority_keypair().to_keypair(),
                curve_mint_keypair,
            ],
            payer: self.tools.solana_keys.authority_keypair().to_keypair(),
        }];

        // We need to lock, it's a heavyweight so add separate tx for that.
        if dev_purchase.is_some() && should_lock {
            let dev_lock_keypair = dev_lock_keypair.ok_or_else(|| {
                anyhow::anyhow!("invariant: no dev lock keypair, but need to lock")
            })?;

            let mut ixs = vec![];
            ixs.append(&mut ix_builder.lock_project()?);
            ixs.append(&mut ix_builder.lock_dev()?);
            ixs.append(&mut ix_builder.init_transmuter()?);
            ixs.append(&mut ix_builder.unlock_project()?);

            txs.push(TransactionRequest {
                instructions: ixs,
                signers: vec![
                    self.tools.solana_keys.authority_keypair().to_keypair(),
                    dev_lock_keypair,
                ],
                payer: self.tools.solana_keys.authority_keypair().to_keypair(),
            })
        }

        if txs.len() == 1 {
            self.tools
                .tx_executor
                .execute_single(txs.into_iter().next().unwrap())
                .await?;
        } else {
            self.tools.tx_executor.execute_batch(txs).await?;
        }

        Ok(())
    }

    async fn graduate_to_raydium(&self) -> anyhow::Result<()> {
        let ix_builder = self
            .tools
            .instructions_builder
            .for_project(&self.lock.project)?;
        let mut first_tx = ix_builder.graduate_curve_pool()?;
        let (openbook_market, mut openbook_market_ix) = ix_builder.prepare_openbook_market()?;
        first_tx.append(&mut openbook_market_ix);

        let jito_meta = self.tools.jito_meta_rx.clone().get()?;
        first_tx.push(jito_meta.tip_ix(&PROGRAM_AUTHORITY));

        let curve_config = self
            .tools
            .moonzip_meta_rx
            .clone()
            .get()?
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
}

struct ChainHelper<'a> {
    pool: &'a SolanaPool,
    fetched_static_pool: Option<StaticPool>,
    project: &'a project::StoredProject,
}

impl<'a> ChainHelper<'a> {
    fn new(pool: &'a SolanaPool, project: &'a project::StoredProject) -> Self {
        Self {
            pool,
            fetched_static_pool: None,
            project,
        }
    }

    async fn should_close_static_pool(&mut self) -> anyhow::Result<bool> {
        let pool = self.static_pool().await?;

        Ok(pool
            .config
            .close_conditions
            .should_be_closed(pool.collected_lamports, TZ::now().timestamp() as u64))
    }

    async fn static_pool(&mut self) -> anyhow::Result<&StaticPool> {
        {
            if let Some(pool) = self.fetched_static_pool.take() {
                self.fetched_static_pool = Some(pool);
                return Ok(self.fetched_static_pool.as_ref().unwrap());
            }
        }
        let mint = self
            .project
            .static_pool_mint()
            .ok_or_else(|| anyhow::anyhow!("invariant: static pool mint is not already stored"))?;

        let address = static_pool_address(mint);
        let pool = self
            .pool
            .rpc_client()
            .use_single()
            .await
            .get_account_data(&address)
            .await
            .context("fetch static pool")?;
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
        self.deploy_metadata(self.lock.project.deploy_schema.curve_pool)
            .await?;
        self.lock.commit().await?;

        Ok(())
    }

    async fn deploy_metadata(&mut self, curve_variant: CurveVariant) -> anyhow::Result<String> {
        let meta = token_meta(&mut self.lock.tx, self.lock.project.id).await?;
        if let Some(deployed_url) = meta.deployed_url {
            return Ok(deployed_url);
        }

        let metadata_uri = {
            let image = token_image(&mut self.lock.tx, self.lock.project.id).await?;

            match curve_variant {
                CurveVariant::Moonzip => {
                    Self::deploy_moonzip_metadata(&self.tools.mzip_ipfs, meta, image).await?
                }
                CurveVariant::Pumpfun => {
                    Self::deploy_pumpfun_metadata(&self.tools.pumpfun_ipfs, meta, image).await?
                }
            }
        };

        sqlx::query!(
            "UPDATE token_meta SET deployed_url = $1 WHERE project_id = $2",
            metadata_uri,
            self.lock.project.id as _,
        )
        .execute(self.lock.tx.deref_mut())
        .await?;

        Ok(metadata_uri)
    }

    async fn deploy_pumpfun_metadata(
        ipfs: &ipfs::pumpfun::PumpfunIpfsClient,
        meta: StoredTokenMeta,
        image: ImageStream<'_>,
    ) -> anyhow::Result<String> {
        let metadata = ipfs::pumpfun::CreateTokenMetadata {
            name: meta.name,
            symbol: meta.symbol,
            description: meta.description,
            image_content: image,
            telegram: meta.telegram,
            website: meta.website,
            twitter: meta.twitter,
        };

        let response = ipfs.deploy_metadata(metadata).await?;

        Ok(response.metadata_uri)
    }

    async fn deploy_moonzip_metadata(
        ipfs: &ipfs::moonzip::IpfsClient,
        meta: StoredTokenMeta,
        image: ImageStream<'_>,
    ) -> anyhow::Result<String> {
        let image_url = ipfs.upload_image(image, &meta.name).await?;
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

        let meta_url = ipfs.upload_json(&metadata, &token_name).await?;
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

    mzip_ipfs: ipfs::moonzip::IpfsClient,
    pumpfun_ipfs: ipfs::pumpfun::PumpfunIpfsClient,
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
                dev_lock_keypair AS "dev_lock_keypair?: _",
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

async fn set_project_stage<'a, 'c, E: Executor<'c, Database = DB> + 'a>(
    executor: E,
    project_id: ProjectId,
    stage: project::Stage,
) -> anyhow::Result<()> {
    query!(
        r#"UPDATE project SET stage = $1 WHERE id = $2"#,
        stage as _,
        project_id as _,
    )
    .execute(executor)
    .await?;
    Ok(())
}

impl<'a> ProjectLock<'a> {
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

async fn token_image<'a, 'b>(
    tx: &'a mut DBTransaction<'b>,
    project_id: ProjectId,
) -> anyhow::Result<ImageStream<'a>> {
    let query = format!(
        "COPY (SELECT image_content FROM token_image WHERE project_id = '{}') TO STDOUT WITH (FORMAT binary)",
        project_id
    );
    let copy_out = tx.copy_out_raw(&query).await?;
    Ok(ImageStream(copy_out))
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
