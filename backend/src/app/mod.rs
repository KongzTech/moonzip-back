use crate::app::exposed::{ChangeUserInfoRequest, GetUserInformationRequest};
use crate::app::exposed::{GetOwnedNFTsRequest, UserInfo};
use crate::app::storage::misc::StoredPubkey;
use crate::solana::SolanaKeys;
use anyhow::bail;
use exposed::{
    BuyRequest, BuyResponse, CreateProjectRequest, CreateProjectResponse, CreateProjectStreamData,
    DevLockClaimRequest, DevLockClaimResponse, DevLockPeriod, GetProjectRequest,
    GetProjectResponse, PublicProject, SellRequest, SellResponse, StoredProjectInfo,
};
use instructions::{
    mpl::{SampleMetadata, SAMPLE_MPL_URI},
    InstructionsBuilder,
};
use rustrict::CensorStr;
use services_common::api::response::ApiError;
use services_common::solana::helius::{GetAssetNFTsResponse, GetOwnedNFTsResponse};
use services_common::solana::pool::SolanaPool;
use services_common::utils::period_fetch::DataReceiver;
use solana_sdk::{signature::Keypair, signer::Signer, transaction::Transaction};
use sqlx::query_as;
use std::{pin::pin, time::Duration};
use storage::user_info::StoredUserInfo;
use storage::{project::StoredProject, StorageClient};
use tokio::io::AsyncRead;
use tracing::debug;
use uuid::Uuid;

pub mod chain_sync;
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
    pub solana_pool: SolanaPool,
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

        if let Some(dev_purchase) = &request.deploy_schema.dev_purchase {
            if !self
                .instructions_builder
                .config
                .allowed_lock_periods
                .contains(&dev_purchase.lock)
            {
                bail!("Invalid dev lock period: {:?}", &dev_purchase.lock)
            }
        }
        let dev_lock_needed = request
            .deploy_schema
            .dev_purchase
            .as_ref()
            .map(|purchase| purchase.lock != DevLockPeriod::Disabled)
            .unwrap_or(false);
        let dev_lock_keypair = if dev_lock_needed {
            Some(Keypair::new().into())
        } else {
            None
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
            dev_lock_keypair,
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
            "INSERT INTO project VALUES ($1, $2, $3, $4, $5, $6, $7, $8)",
            project.id,
            project.owner as _,
            project.deploy_schema as _,
            project.stage as _,
            project.static_pool_pubkey as _,
            project.curve_pool_keypair as _,
            project.dev_lock_keypair as _,
            project.created_at
        )
        .execute(&mut *tx)
        .await?;

        sqlx::query!("INSERT INTO project_migration_lock VALUES ($1)", project.id)
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
        let ixs = builder.buy(request.user, request.sols, request.min_token_output)?;
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

    pub async fn dev_lock_claim(
        &self,
        request: DevLockClaimRequest,
    ) -> anyhow::Result<DevLockClaimResponse> {
        let project = self.get_full_project(request.project_id).await?;

        let builder = self.instructions_builder.for_project(&project)?;
        let ixs = builder.claim_dev_lock()?;
        let tx = Transaction::new_with_payer(&ixs, Some(&project.owner.to_pubkey()));

        Ok(DevLockClaimResponse { transaction: tx })
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
                project.dev_lock_keypair AS "dev_lock_keypair?: _",
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
                dev_lock_keypair AS "dev_lock_keypair?: _",
                created_at
            FROM project WHERE id = $1"#,
            project_id as _,
        )
        .fetch_one(&self.storage.pool)
        .await?;
        Ok(project)
    }

    pub async fn upsert_user_info(
        &self,
        request: ChangeUserInfoRequest,
    ) -> anyhow::Result<UserInfo, ApiError> {
        let mut image_url: String = String::from("");
        let mut stored_nft_address: Option<StoredPubkey> = None;
        if request.nft_address.is_some() {
            let response = self
                .solana_pool
                .helius_client()
                .get_nft_info(
                    request.nft_address.unwrap().to_string(),
                    request.wallet_address.to_string(),
                )
                .await?;
            if self
                .retrieve_owner_address_from_asset(response.clone())
                .await?
                != request.wallet_address.to_string()
            {
                return Err(ApiError::NFTNotBelong2User(anyhow::anyhow!(
                    "NFT doesn't belong to this user"
                )));
            }
            image_url = self.retrieve_image_url_from_asset(response.clone()).await?;
            stored_nft_address = request.nft_address.map(|pubkey| pubkey.into());
        }

        let stored_user_info = StoredUserInfo {
            wallet_address: request.wallet_address.into(),
            username: request.username,
            display_name: Some(request.display_value),
            image_url: Some(image_url),
            nft_address: stored_nft_address,
            last_active: None,
            created_at: Some(chrono::Utc::now()),
            updated_at: Some(chrono::Utc::now()),
        };

        if CensorStr::is_inappropriate(&*stored_user_info.username) {
            return Err(ApiError::InvalidUsernameFormat(anyhow::anyhow!(
                "Username contains bad words"
            )));
        }

        let tx = self.storage.serializable_tx().await?;
        let updated_user = sqlx::query_as!(
                StoredUserInfo,
                r#"
                    INSERT INTO user_info (wallet_address, username, image_url, nft_address, created_at, updated_at)
                    VALUES ($1, $2, $3, $4, now(), now())
                    ON CONFLICT (wallet_address)
                    DO UPDATE SET
                        username = EXCLUDED.username,
                        image_url = EXCLUDED.image_url,
                        nft_address = EXCLUDED.nft_address,
                        updated_at = now()
                    RETURNING wallet_address as "wallet_address: _", username, image_url, nft_address as "nft_address?: _", display_name, last_active, created_at, updated_at
                "#,
                stored_user_info.wallet_address as _,
                stored_user_info.username as _,
                stored_user_info.image_url as _,
                stored_user_info.nft_address as _
            )
            .fetch_one(&self.storage.pool)
            .await
            .map_err(|_e| {
                ApiError::ExistedUsername(anyhow::anyhow!(
                "Failed to retrieve updated user information for wallet: {}",
                request.wallet_address))
            })?;

        tx.commit()
            .await
            .map_err(|_e| ApiError::Internal(anyhow::anyhow!("Failed to commit transaction")))?;

        let user_info = UserInfo::try_from(updated_user).map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("Failed to convert stored user info: {}", e))
        })?;

        Ok(user_info)
    }

    pub async fn get_user_info_by_address(
        &self,
        request: GetUserInformationRequest,
    ) -> anyhow::Result<UserInfo, ApiError> {
        let stored_pub_key: StoredPubkey = request.wallet_address.into();

        let user = query_as!(
            StoredUserInfo,
            r#"select  u.wallet_address as "wallet_address: _",
                       u.username,
                       u.display_name,
                       u.image_url,
                       u.nft_address AS "nft_address?: _",
                       u.last_active,
                       u.created_at,
                       u.updated_at
                from user_info u
                where wallet_address = $1"#,
            stored_pub_key as _
        )
        .fetch_one(&self.storage.pool)
        .await
        .map_err(|_e| ApiError::NotFoundUser(anyhow::anyhow!("Not found user by address",)))?;

        let user_info = UserInfo::try_from(user).map_err(|e| {
            ApiError::Internal(anyhow::anyhow!("Failed to convert stored user info: {}", e))
        })?;

        Ok(user_info)
    }

    pub async fn get_owned_nfts_by_address(
        &self,
        request: GetOwnedNFTsRequest,
    ) -> anyhow::Result<GetOwnedNFTsResponse> {
        self.solana_pool
            .helius_client()
            .get_owned_nfts(
                request.owner_address.to_string(),
                request.page,
                request.limit,
            )
            .await
    }

    async fn retrieve_image_url_from_asset(
        &self,
        asset_response: GetAssetNFTsResponse,
    ) -> anyhow::Result<String> {
        asset_response
            .result
            .unwrap()
            .content
            .links
            .image
            .ok_or_else(|| anyhow::anyhow!("Image url is missing from response"))
    }

    async fn retrieve_owner_address_from_asset(
        &self,
        asset_response: GetAssetNFTsResponse,
    ) -> anyhow::Result<String> {
        asset_response
            .result
            .unwrap()
            .ownership
            .owner
            .ok_or_else(|| anyhow::anyhow!("Owner address is missing from response"))
    }
}
