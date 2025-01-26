use super::storage::project::{project_id, CurveVariant, StoredProject, StoredTokenMeta};
use anchor_spl::associated_token::get_associated_token_address;
use anyhow::bail;
use moonzip::{
    accounts::BaseInitTransmuterAccounts,
    common::PoolCloseConditions,
    moonzip::{
        curve, static_pool_address, BuyFromCurvedPoolData, CreateStaticPoolData, StaticPoolConfig,
        CURVED_POOL_PREFIX, TRANSMUTER_PREFIX,
    },
    project::{project_address, CreateProjectData},
    PROGRAM_AUTHORITY,
};
use mpl_token_metadata::instructions::CreateV1Builder;
use openbook::OpenbookInstructionsBuilder;
use serde::Deserialize;
use serum_dex::instruction::initialize_market;
use services_common::{solana::pool::SolanaPool, utils::period_fetch::DataReceiver, TZ};
use solana_sdk::{
    instruction::Instruction,
    native_token::{sol_to_lamports, LAMPORTS_PER_SOL},
    pubkey::Pubkey,
    rent::Rent,
    signature::Keypair,
    signer::Signer as _,
};
use std::{str::FromStr, sync::Arc, time::Duration};

mod mpl;
pub mod mzip;
pub mod openbook;
pub mod pumpfun;
pub mod raydium;
pub mod solana;

#[derive(Debug, Clone, Deserialize)]
pub struct InstructionsConfig {
    #[serde(default = "default_serum_openbook_program")]
    pub serum_openbook_program: Pubkey,
    #[serde(default = "default_sols_to_graduate")]
    pub sols_to_graduate: u64,
    #[serde(default = "default_rayidum_liquidity")]
    pub rayidum_liquidity: u64,
    #[serde(default = "default_creator_graduate_reward")]
    pub creator_graduate_reward: u64,
}

fn default_serum_openbook_program() -> Pubkey {
    Pubkey::from_str("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX").unwrap()
}

fn default_sols_to_graduate() -> u64 {
    LAMPORTS_PER_SOL * 85
}

fn default_rayidum_liquidity() -> u64 {
    LAMPORTS_PER_SOL * 79
}

fn default_creator_graduate_reward() -> u64 {
    sol_to_lamports(0.5)
}

const WRAPPED_SOL_MINT: Pubkey = solana_sdk::pubkey!("So11111111111111111111111111111111111111112");
#[derive(Clone)]
pub struct InstructionsBuilder {
    pub solana_pool: SolanaPool,
    pub solana_meta: DataReceiver<solana::Meta>,
    pub config: Arc<InstructionsConfig>,
}

impl InstructionsBuilder {
    pub async fn for_project<'a>(
        &'a self,
        project: &'a StoredProject,
    ) -> anyhow::Result<ProjectsOperations> {
        Ok(ProjectsOperations {
            solana_pool: &self.solana_pool,
            project,
            config: &self.config,
            rent: self.solana_meta.clone().get().await?.rent,
        })
    }
}

#[derive(Clone)]
pub struct ProjectsOperations<'a> {
    solana_pool: &'a SolanaPool,
    project: &'a StoredProject,
    config: &'a InstructionsConfig,
    rent: Rent,
}

impl<'a> ProjectsOperations<'a> {
    pub fn create_project(&mut self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project.id);
        let project_address = project_address(&project_id);

        let ix = program
            .request()
            .accounts(moonzip::accounts::CreateProjectAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                creator: self.project.owner.clone().into(),
                project: project_address,
                system_program: solana_sdk::system_program::ID,
            })
            .args(moonzip::instruction::CreateProject {
                data: CreateProjectData {
                    id: project_id,
                    schema: self.project.deploy_schema.to_project_schema(),
                },
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn init_static_pool(
        &mut self,
        static_pool_mint: &Keypair,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project.id);
        let project_address = project_address(&project_id);

        let pool_address = static_pool_address(static_pool_mint.pubkey());

        let pool_mint_account =
            get_associated_token_address(&pool_address, &static_pool_mint.pubkey());

        let finish_after = Duration::from_secs(self.project.deploy_schema.launch_after as u64);
        let finish_ts = (TZ::now() + finish_after).timestamp();

        let ix = program
            .request()
            .accounts(moonzip::accounts::CreateStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address,
                mint: static_pool_mint.pubkey(),
                pool_mint_account,
                pool: pool_address,

                system_program: solana_sdk::system_program::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: anchor_spl::token::ID,
            })
            .args(moonzip::instruction::CreateStaticPool {
                data: CreateStaticPoolData {
                    project_id,
                    config: StaticPoolConfig {
                        min_purchase_lamports: None,
                        close_conditions: PoolCloseConditions {
                            finish_ts: Some(finish_ts as u64),
                            max_lamports: Some(self.config.sols_to_graduate),
                        },
                    },
                },
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn graduate_static_pool(&mut self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let static_pool_mint = Keypair::new();

        let pool_address = static_pool_address(static_pool_mint.pubkey());

        let ix = program
            .request()
            .accounts(moonzip::accounts::GraduateStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                funds_receiver: moonzip::PROGRAM_AUTHORITY,
                pool: pool_address,

                system_program: solana_sdk::system_program::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: anchor_spl::token::ID,
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn add_transmuter_for_moonzip(
        &mut self,
        args: TransmuterInitArgs,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let curved_pool = get_curved_pool_address(args.from_mint);
        let base = self.base_transmuter_init_accounts(&args);
        let ix = program
            .request()
            .accounts(moonzip::accounts::InitTransmuterForCurveAccounts { base, curved_pool })
            .instructions()?;

        Ok(ix)
    }

    pub fn add_transmuter_for_pumpfun(
        &mut self,
        args: TransmuterInitArgs,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(pumpfun_cpi::ID)?;
        let bonding_curve = pumpfun::get_bonding_curve(&args.from_mint);

        let ix = program
            .request()
            .accounts(moonzip::accounts::InitTransmuterForPumpfunCurveAccounts {
                base: self.base_transmuter_init_accounts(&args),
                bonding_curve,
            })
            .instructions()?;

        Ok(ix)
    }

    fn base_transmuter_init_accounts(
        &self,
        args: &TransmuterInitArgs,
    ) -> BaseInitTransmuterAccounts {
        let transmuter = get_transmuter_address(args.from_mint, args.to_mint);

        BaseInitTransmuterAccounts {
            authority: moonzip::PROGRAM_AUTHORITY,
            from_mint: args.from_mint,
            to_mint: args.to_mint,
            donor_to_mint_account: get_associated_token_address(&args.to_mint, &args.donor),
            donor: args.donor,
            transmuter_to_mint_account: get_associated_token_address(&transmuter, &args.donor),
            transmuter,

            system_program: solana_sdk::system_program::ID,
            associated_token_program: anchor_spl::associated_token::ID,
            token_program: anchor_spl::token::ID,
        }
    }

    pub fn init_pumpfun_pool(
        &mut self,
        action: CurveCreate,
        pumpfun_meta: pumpfun::Meta,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(pumpfun_cpi::ID)?;
        let bonding_curve = pumpfun::get_bonding_curve(&action.mint);
        let associated_bonding_curve = get_associated_token_address(&action.mint, &bonding_curve);
        let initial_curve = moonzip::pumpfun::CurveWrapper::initial(&pumpfun_meta.global_account);
        let tokens_to_buy = moonzip::pumpfun::BuyCalculator::new(&initial_curve)
            .fixed_sols(action.initial_purchase.amount);

        let mut result = vec![];
        let mut create_ixs = program
            .request()
            .accounts(pumpfun_cpi::accounts::Create {
                mint: action.mint,
                mint_authority: *pumpfun::MINT_AUTHORITY,
                bonding_curve,
                associated_bonding_curve,
                global: *pumpfun::GLOBAL,
                metadata: mpl::metadata_account(action.mint),
                user: self.project.owner.clone().into(),
                event_authority: pumpfun::EVENT_AUTHORITY,

                program: pumpfun_cpi::ID,
                mpl_token_metadata: *mpl::PROGRAM,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                rent: solana_sdk::sysvar::rent::ID,
            })
            .args(pumpfun_cpi::instruction::Create {
                _name: action.metadata.name.clone(),
                _symbol: action.metadata.symbol.clone(),
                _uri: action.metadata.deployed_url()?,
            })
            .instructions()?;
        result.append(&mut create_ixs);
        let mut ixs = program
            .request()
            .accounts(pumpfun_cpi::accounts::Buy {
                global: *pumpfun::GLOBAL,
                event_authority: pumpfun::EVENT_AUTHORITY,
                fee_recipient: pumpfun_meta.global_account.fee_recipient,

                mint: action.mint,
                bonding_curve,
                associated_bonding_curve,

                associated_user: get_associated_token_address(
                    &action.initial_purchase.user,
                    &action.mint,
                ),
                user: action.initial_purchase.user,

                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                rent: solana_sdk::sysvar::rent::ID,
                program: pumpfun_cpi::ID,
            })
            .args(pumpfun_cpi::instruction::Buy {
                _amount: tokens_to_buy,
                _max_sol_cost: action.initial_purchase.amount,
            })
            .instructions()?;
        result.append(&mut ixs);

        Ok(result)
    }

    pub fn init_moonzip_pool(
        &mut self,
        action: CurveCreate,
        meta: &mzip::Meta,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let pool_address = get_curved_pool_address(action.mint);
        let project = self.get_project_address();
        let pool_token_account = get_associated_token_address(&pool_address, &action.mint);

        let mut ix = program
            .request()
            .accounts(moonzip::accounts::CreateCurvedPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project,

                global: *mzip::GLOBAL_ACCOUNT,
                mint: action.mint,
                pool_token_account,
                pool: pool_address,

                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .instructions()?;

        let initial_curve = curve::CurveState::from_cfg(&meta.global_account.config.curve);
        let tokens_to_buy =
            curve::SellCalculator::new(&initial_curve).fixed_sols(action.initial_purchase.amount);

        ix.append(
            &mut program
                .request()
                .accounts(moonzip::accounts::BuyFromCurvedPoolAccounts {
                    authority: moonzip::PROGRAM_AUTHORITY,
                    project,
                    mint: action.mint,

                    pool_token_account,
                    pool: pool_address,

                    user_token_account: get_associated_token_address(
                        &action.initial_purchase.user,
                        &action.mint,
                    ),
                    user: action.initial_purchase.user,

                    system_program: solana_sdk::system_program::ID,
                    token_program: anchor_spl::token::ID,
                    associated_token_program: anchor_spl::associated_token::ID,
                })
                .args(moonzip::instruction::BuyFromCurvedPool {
                    data: BuyFromCurvedPoolData {
                        project_id: project_id(&self.project.id),
                        tokens: tokens_to_buy,
                        max_sol_cost: action.initial_purchase.amount,
                    },
                })
                .instructions()?,
        );

        ix.push(
            CreateV1Builder::new()
                .metadata(mpl::metadata_account(action.mint))
                .mint(action.mint, true)
                .authority(moonzip::PROGRAM_AUTHORITY)
                .payer(moonzip::PROGRAM_AUTHORITY)
                .update_authority(moonzip::PROGRAM_AUTHORITY, true)
                .is_mutable(false)
                .primary_sale_happened(false)
                .name(action.metadata.name.clone())
                .uri(action.metadata.deployed_url()?)
                .seller_fee_basis_points(0)
                .token_standard(mpl_token_metadata::types::TokenStandard::Fungible)
                .instruction(),
        );

        Ok(ix)
    }

    pub fn graduate_curve_pool(&self) -> anyhow::Result<Vec<Instruction>> {
        if matches!(self.project.deploy_schema.curve_pool, CurveVariant::Pumpfun) {
            bail!("pumpfun curve pools could not be graduated");
        }

        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let pool_address = self.curve_mint()?;

        let ix = program
            .request()
            .accounts(moonzip::accounts::GraduateCurvedPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                funds_receiver: moonzip::PROGRAM_AUTHORITY,
                pool: pool_address,

                system_program: solana_sdk::system_program::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: anchor_spl::token::ID,
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn prepare_openbook_market(&self) -> anyhow::Result<(Pubkey, Vec<Instruction>)> {
        let curve_mint = self.curve_mint()?;
        let builder = OpenbookInstructionsBuilder {
            rent: &self.rent,
            payer: &PROGRAM_AUTHORITY,
            mint: &curve_mint,
            program_id: &self.config.serum_openbook_program,
        };
        let (market, create_market_ix) = builder.create_market_account()?;

        let (request_queue, request_queue_ix) = builder.create_queue_account("request", 764)?;
        let (event_queue, event_queue_ix) = builder.create_queue_account("event", 11308)?;
        let (bids, bids_ix) = builder.create_queue_account("bids", 14524)?;
        let (asks, asks_ix) = builder.create_queue_account("asks", 14524)?;

        let coin_mint_pk = &WRAPPED_SOL_MINT;
        let coin_vault_pk = get_associated_token_address(&market, coin_mint_pk);

        let pc_mint_pk = curve_mint;
        let pc_vault_pk = get_associated_token_address(&market, &pc_mint_pk);

        let initialize_market_ix = initialize_market(
            &market,
            &self.config.serum_openbook_program,
            coin_mint_pk,
            &pc_mint_pk,
            &coin_vault_pk,
            &pc_vault_pk,
            None,
            None,
            None,
            &bids,
            &asks,
            &request_queue,
            &event_queue,
            6447184,
            64,
            0,
            64,
        )?;

        Ok((
            market,
            vec![
                create_market_ix,
                bids_ix,
                asks_ix,
                request_queue_ix,
                event_queue_ix,
                initialize_market_ix,
            ],
        ))
    }

    pub fn lock_project(&self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project.id);
        let project_address = project_address(&project_id);

        let ix = program
            .request()
            .accounts(moonzip::accounts::ProjectLockLatchAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address,
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn unlock_project(&self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project.id);
        let project_address = project_address(&project_id);

        let ix = program
            .request()
            .accounts(moonzip::accounts::ProjectUnlockLatchAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address,
            })
            .instructions()?;

        Ok(ix)
    }

    fn get_project_address(&self) -> Pubkey {
        project_address(&project_id(&self.project.id))
    }

    fn curve_mint(&self) -> anyhow::Result<Pubkey> {
        Ok(self
            .project
            .curve_pool_keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("invariant: no curve mint"))?
            .to_keypair()
            .pubkey())
    }
}

#[derive(Debug, Clone)]
pub struct CurveCreate {
    pub mint: Pubkey,
    pub initial_purchase: InitialPurchase,
    pub metadata: StoredTokenMeta,
}

#[derive(Debug, Clone)]
pub struct InitialPurchase {
    pub user: Pubkey,
    pub amount: u64,
}

pub struct TransmuterInitArgs {
    pub from_mint: Pubkey,
    pub to_mint: Pubkey,
    pub donor: Pubkey,
}

fn get_transmuter_address(from_mint: Pubkey, to_mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(
        &[TRANSMUTER_PREFIX, from_mint.as_ref(), to_mint.as_ref()],
        &moonzip::ID,
    )
    .0
}

fn get_curved_pool_address(mint: Pubkey) -> Pubkey {
    Pubkey::find_program_address(&[CURVED_POOL_PREFIX, mint.as_ref()], &moonzip::ID).0
}
