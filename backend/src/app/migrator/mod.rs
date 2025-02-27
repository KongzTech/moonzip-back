use super::{
    instructions::{mzip, pumpfun, CurveCreate, InitialPurchase, InstructionsBuilder},
    storage::{
        project::{self, CurveVariant, FullProjectState, ImageStream, ProjectId, StoredTokenMeta},
        DBTransaction, StorageClient,
    },
};
use crate::solana::SolanaKeys;
use anyhow::bail;
use chrono::DateTime;
use const_format::concatcp;
use derive_more::derive::Deref;
use moonzip::PROGRAM_AUTHORITY;
use rust_decimal::prelude::Zero;
use serde::{Deserialize, Serialize};
use services_common::{
    solana::{jito, pool::SolanaPool},
    utils::period_fetch::{DataReceiver, PeriodicFetcher, PeriodicFetcherConfig},
    TZ,
};
use solana_sdk::signer::Signer;
use sqlx::{query, query_as};
use std::{ops::DerefMut, sync::Arc, time::Duration};
use tokio::{spawn, task::JoinHandle};
use tracing::{debug, error, info, instrument, warn};
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
                } else {
                    debug!("migrator tick completed successfully");
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
        let eligible_stages = [
            project::Stage::Confirmed,
            project::Stage::OnStaticPool,
            project::Stage::StaticPoolClosed,
            project::Stage::CurvePoolClosed,
        ];
        let projects:Vec<FullProjectState> = query_as(
            concatcp!(
                FullProjectState::QUERY_BODY,
                "WHERE stage = ANY($1::project_stage[]) AND created_at > $2 ORDER BY created_at ASC LIMIT $3"
            )
        ).bind(eligible_stages).bind(after).bind(Self::PAGE_SIZE as i64)
        .fetch_all(&*self.tools.storage)
        .await?;
        debug!(
            "received projects for migration: {:?}",
            projects
                .iter()
                .map(|project| project.project.id)
                .collect::<Vec<_>>()
        );
        let received_projects = projects.len();
        let last_timemark = projects.last().map(|project| project.project.created_at);

        for project in projects.into_iter() {
            let executor = ProjectMigrationExecutor {
                tools: self.tools.clone(),
                project_state: project,
            };
            spawn(async move {
                let id = executor.project_state.project.id;
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

struct ProjectMigrationExecutor {
    tools: Tools,
    project_state: FullProjectState,
}

impl ProjectMigrationExecutor {
    #[instrument(skip(self), fields(project_id = %self.project_state.project.id))]
    async fn migrate(self) -> anyhow::Result<()> {
        match self.project_state.project.stage {
            // project is created with graduation straight to curve pool
            project::Stage::Confirmed => {
                if self
                    .project_state
                    .project
                    .deploy_schema
                    .static_pool
                    .is_some()
                {
                    bail!("invariant: project must have begun straight to static pool");
                }
                info!("would deploy curve, avoiding static pool");
                self.deploy_curve().await?;
            }
            // we need to migrate static pool to curve pool
            project::Stage::StaticPoolClosed => {
                info!("would deploy curve, static pool is already closed");
                self.deploy_curve().await?;
            }
            project::Stage::OnStaticPool => {
                if self.project_state.should_close_static_pool() {
                    info!("would deploy curve, static pool should be closed by time");
                    self.deploy_curve().await?;
                }
            }
            project::Stage::CurvePoolClosed => {
                info!("curve pool closed, need to deploy on raydium");
                let mut lock = self
                    .tools
                    .lock_project(&self.project_state.project.id)
                    .await?;
                let deployer = Deployer {
                    tools: self.tools.clone(),
                    lock: &mut lock,
                    project_state: &self.project_state,
                };
                deployer.graduate_to_raydium().await?;
            }
            _ => {
                bail!("invariant: other stage must not propagate to the migrator");
            }
        }
        Ok(())
    }

    async fn deploy_curve<'a>(&self) -> anyhow::Result<()> {
        let mut lock = self
            .tools
            .lock_project(&self.project_state.project.id)
            .await?;

        // Assign keypair and deploy project metadata if needed
        if self.project_state.project.curve_pool_mint().is_none() {
            let prepare_curve_deploy = PrepareCurveDeploy {
                tools: self.tools.clone(),
                project_state: &self.project_state,
                lock,
            };
            prepare_curve_deploy.prepare().await?;
            lock = self
                .tools
                .lock_project(&self.project_state.project.id)
                .await?;
        }

        let mut deployer = Deployer {
            tools: self.tools.clone(),
            lock: &mut lock,
            project_state: &self.project_state,
        };
        deployer.init_curve_pool().await?;
        Ok(())
    }
}

struct Deployer<'a, 'b> {
    tools: Tools,
    lock: &'a mut ProjectLock<'b>,
    project_state: &'a FullProjectState,
}

impl<'a, 'b> Deployer<'a, 'b> {
    async fn init_curve_pool(&mut self) -> anyhow::Result<()> {
        let dev_purchase = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .cloned()
            .map(|purchase| purchase.amount.try_into())
            .transpose()?;

        let post_dev_purchase = if self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_some()
        {
            let collected: u64 = self
                .project_state
                .static_pool_state
                .as_ref()
                .map(|state| state.collected_lamports.clone())
                .unwrap_or_default()
                .try_into()?;
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
            .for_project(self.project_state)?;

        let curve_mint_keypair = self
            .project_state
            .project
            .curve_pool_keypair
            .as_ref()
            .map(|keypair| keypair.to_keypair())
            .ok_or_else(|| {
                anyhow::anyhow!("invariant: curve pool secret key is not already stored")
            })?;
        let dev_lock_keypair = self
            .project_state
            .project
            .dev_lock_keypair
            .as_ref()
            .map(|keypair| keypair.to_keypair());
        let should_lock = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .map(|purchase| !purchase.lock_period.is_zero())
            .unwrap_or(false);

        let token_meta = token_meta(&mut self.lock.tx, self.project_state.project.id).await?;
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

        let mut first_tx = TransactionRequest {
            instructions: vec![],
            signers: vec![
                self.tools.solana_keys.authority_keypair().to_keypair(),
                curve_mint_keypair,
            ],
            payer: self.tools.solana_keys.authority_keypair().to_keypair(),
        };

        first_tx
            .instructions
            .append(&mut ix_builder.lock_project()?);

        if self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_some()
        {
            first_tx
                .instructions
                .append(&mut ix_builder.graduate_static_pool()?);
        }

        match self.project_state.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => {
                first_tx
                    .instructions
                    .append(&mut ix_builder.init_moonzip_pool(curve_create)?);
            }
            CurveVariant::Pumpfun => {
                let pumpfun_meta = self.tools.pumpfun_meta_rx.clone().get()?;
                first_tx
                    .instructions
                    .append(&mut ix_builder.init_pumpfun_pool(curve_create, pumpfun_meta)?);
            }
        };
        first_tx
            .instructions
            .append(&mut ix_builder.unlock_project()?);

        // second transaction is for tokens delivery mainly.
        let mut second_tx = TransactionRequest {
            instructions: vec![],
            signers: vec![self.tools.solana_keys.authority_keypair().to_keypair()],
            payer: self.tools.solana_keys.authority_keypair().to_keypair(),
        };
        second_tx
            .instructions
            .append(&mut ix_builder.lock_project()?);
        if should_lock {
            second_tx.instructions.append(&mut ix_builder.lock_dev()?);
            second_tx.signers.push(
                dev_lock_keypair
                    .ok_or_else(|| anyhow::anyhow!("no dev lock keypair, but need to lock"))?,
            );
        } else {
            second_tx
                .instructions
                .append(&mut ix_builder.deliver_dev_tokens()?);
        }
        second_tx
            .instructions
            .append(&mut ix_builder.init_transmuter()?);
        second_tx
            .instructions
            .append(&mut ix_builder.unlock_project()?);

        let txs = vec![first_tx, second_tx];
        self.tools.tx_executor.execute_batch(txs).await?;

        Ok(())
    }

    async fn graduate_to_raydium(&self) -> anyhow::Result<()> {
        let ix_builder = self
            .tools
            .instructions_builder
            .for_project(self.project_state)?;
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

struct PrepareCurveDeploy<'a> {
    tools: Tools,
    lock: ProjectLock<'a>,
    project_state: &'a FullProjectState,
}

impl<'a> PrepareCurveDeploy<'a> {
    async fn prepare(mut self) -> anyhow::Result<()> {
        query!(
            r#"call assign_project_keypair($1)"#,
            self.project_state.project.id as _,
        )
        .execute(&mut *self.lock.tx)
        .await?;
        self.deploy_metadata(self.project_state.project.deploy_schema.curve_pool)
            .await?;
        self.lock.commit().await?;

        Ok(())
    }

    async fn deploy_metadata(&mut self, curve_variant: CurveVariant) -> anyhow::Result<String> {
        let meta = token_meta(&mut self.lock.tx, self.project_state.project.id).await?;
        if let Some(deployed_url) = meta.deployed_url {
            return Ok(deployed_url);
        }

        let metadata_uri = {
            let image = token_image(&mut self.lock.tx, self.project_state.project.id).await?;

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
            self.project_state.project.id as _,
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

        sqlx::query!(
            "SELECT id FROM project_migration_lock WHERE id = $1 FOR UPDATE NOWAIT;",
            project_id
        )
        .fetch_one(tx.deref_mut())
        .await?;

        Ok(ProjectLock { tx })
    }
}

struct ProjectLock<'a> {
    tx: DBTransaction<'a>,
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
