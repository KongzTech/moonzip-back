use super::{
    exposed::DevLockPeriod,
    storage::project::{project_id, CurveVariant, FullProjectState, Stage, StoredTokenMeta},
};
use anchor_spl::associated_token::{
    self, get_associated_token_address,
    spl_associated_token_account::instruction::{
        create_associated_token_account, create_associated_token_account_idempotent,
    },
};
use anyhow::bail;
use moonzip::{
    accounts::BaseInitTransmuterAccounts,
    common::PoolCloseConditions,
    fee::fee_address,
    instruction::{BuyFromPump, GraduateStaticPool},
    moonzip::{
        curve::CalcBuy as _, curved_pool_address, static_pool_address, BuyFromCurvedPoolData,
        BuyFromStaticPoolData, CreateCurvedPoolData, CreateStaticPoolData, CurvedPool,
        GraduateCurvedPoolData, SellFromCurvedPoolData, SellToStaticPoolData, StaticPool,
        StaticPoolConfig, Transmuter, CURVED_POOL_PREFIX, TRANSMUTER_PREFIX,
    },
    project::{project_address, CreateProjectData},
    pumpfun::BuyFromPumpData,
    PROGRAM_AUTHORITY,
};
use mpl::SampleMetadata;
use mpl_token_metadata::instructions::CreateV1Builder;
use mzip::{FEE_ACCOUNT, MOONZIP_EVENT_AUTHORITY};
use serde::Deserialize;
use serde_with::{serde_as, DurationSeconds};
use services_common::{solana::pool::SolanaPool, utils::period_fetch::DataReceiver, TZ};
use sha2::{Digest, Sha256};
use solana_sdk::{
    instruction::Instruction,
    native_token::{sol_to_lamports, LAMPORTS_PER_SOL},
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    signature::Keypair,
    signer::Signer as _,
    system_instruction,
};
use std::{str::FromStr, sync::Arc, time::Duration};
use utils::anchor_event_authority;

pub mod lock;
pub mod mpl;
pub mod mzip;
pub mod openbook;
pub mod pumpfun;
pub mod raydium;
pub mod solana;
pub mod utils;

#[serde_as]
#[derive(Debug, Clone, Deserialize, serde_derive_default::Default)]
pub struct InstructionsConfig {
    #[serde(default = "default_serum_openbook_program")]
    pub serum_openbook_program: Pubkey,

    #[serde(default = "default_locker_program")]
    pub locker_program: Pubkey,

    #[serde(default = "default_raydium_program")]
    pub raydium_program: Pubkey,

    #[serde(default = "default_memo_program")]
    pub memo_program: Pubkey,

    #[serde(default = "default_sols_to_graduate")]
    pub sols_to_graduate: u64,
    #[serde(default = "default_raydium_liquidity")]
    pub raydium_liquidity: u64,
    #[serde(default = "default_creator_graduate_reward")]
    pub creator_graduate_reward: u64,
    #[serde(default = "default_pumpfun_init_price")]
    pub pumpfun_init_price: u64,

    #[serde(default = "default_allowed_launch_periods")]
    #[serde_as(as = "Vec<DurationSeconds<u64>>")]
    pub allowed_launch_periods: Vec<Duration>,

    #[serde(default = "default_allowed_lock_periods")]
    pub allowed_lock_periods: Vec<DevLockPeriod>,
}

fn default_allowed_launch_periods() -> Vec<Duration> {
    vec![
        Duration::from_secs(60 * 60),
        Duration::from_secs(12 * 60 * 60),
        Duration::from_secs(24 * 60 * 60),
    ]
}

fn default_allowed_lock_periods() -> Vec<DevLockPeriod> {
    let hour = 60 * 60;
    vec![
        DevLockPeriod::Disabled,
        DevLockPeriod::Interval {
            interval: hour * 24,
        },
        DevLockPeriod::Interval {
            interval: hour * 24 * 7,
        },
        DevLockPeriod::Interval {
            interval: hour * 24 * 30,
        },
    ]
}

fn default_raydium_program() -> Pubkey {
    Pubkey::from_str("675kPX9MHTjS2zt1qfr1NYHuzeLXfQM9H24wFSUt1Mp8").unwrap()
}

fn default_locker_program() -> Pubkey {
    Pubkey::from_str("LocpQgucEQHbqNABEYvBvwoxCPsSbG91A1QaQhQQqjn").unwrap()
}

fn default_serum_openbook_program() -> Pubkey {
    Pubkey::from_str("srmqPvymJeFKQ4zGQed1GFppgkRHL9kaELCbyksJtPX").unwrap()
}

fn default_memo_program() -> Pubkey {
    Pubkey::from_str("MemoSq4gqABAXKb96qnH8TysNcWxMyWCqXgDLGmfcHr").unwrap()
}

fn default_sols_to_graduate() -> u64 {
    LAMPORTS_PER_SOL * 85
}

fn default_raydium_liquidity() -> u64 {
    LAMPORTS_PER_SOL * 79
}

fn default_creator_graduate_reward() -> u64 {
    sol_to_lamports(0.5)
}

fn default_pumpfun_init_price() -> u64 {
    sol_to_lamports(0.022)
}

const WRAPPED_SOL_MINT: Pubkey = solana_sdk::pubkey!("So11111111111111111111111111111111111111112");

#[derive(Clone)]
pub struct InstructionsBuilder {
    pub solana_pool: SolanaPool,
    pub solana_meta: DataReceiver<solana::Meta>,
    pub pump_meta: DataReceiver<pumpfun::Meta>,
    pub mzip_meta: DataReceiver<mzip::Meta>,
    pub config: Arc<InstructionsConfig>,
}

impl InstructionsBuilder {
    pub fn for_project<'a>(
        &'a self,
        project: &'a FullProjectState,
    ) -> anyhow::Result<ProjectsOperations> {
        Ok(ProjectsOperations {
            solana_pool: &self.solana_pool,
            project_state: project,
            config: &self.config,

            pump_meta: self.pump_meta.clone(),
            mzip_meta: self.mzip_meta.clone(),

            rent: self.solana_meta.clone().get()?.rent,
        })
    }
}

#[derive(Clone)]
pub struct ProjectsOperations<'a> {
    solana_pool: &'a SolanaPool,
    project_state: &'a FullProjectState,
    config: &'a InstructionsConfig,

    pump_meta: DataReceiver<pumpfun::Meta>,
    mzip_meta: DataReceiver<mzip::Meta>,

    rent: Rent,
}

impl<'a> ProjectsOperations<'a> {
    pub fn create_project(
        &mut self,
        metadata: SampleMetadata<'a>,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        // will certainly need for main token
        let mut creator_deposit = 0;

        // if static pool, will need for pool mint and static pool itself
        if self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_some()
        {
            creator_deposit += self.rent.minimum_balance(StaticPool::ACCOUNT_SIZE);
            creator_deposit += self.rent.minimum_balance(Transmuter::ACCOUNT_SIZE);
            creator_deposit += self.rent.minimum_balance(spl_token::state::Account::LEN) * 2;
            creator_deposit += self.rent.minimum_balance(spl_token::state::Mint::LEN);
        }

        match self.project_state.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => {
                creator_deposit += self.rent.minimum_balance(spl_token::state::Mint::LEN);
                creator_deposit += self.rent.minimum_balance(spl_token::state::Account::LEN);
                creator_deposit += self.rent.minimum_balance(CurvedPool::ACCOUNT_SIZE);
                creator_deposit += metadata.estimate_price(&self.rent)?;
            }
            CurveVariant::Pumpfun => {
                creator_deposit += self.config.pumpfun_init_price;
            }
        }

        if let Some(purchase) = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .clone()
        {
            creator_deposit += u64::try_from(purchase.amount)?;
        }

        let ix = program
            .request()
            .accounts(moonzip::accounts::CreateProjectAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                event_authority: *MOONZIP_EVENT_AUTHORITY,
                creator: self.project_state.project.owner.clone().into(),
                project: project_address,
                system_program: solana_sdk::system_program::ID,
                program: moonzip::ID,
            })
            .args(moonzip::instruction::CreateProject {
                data: CreateProjectData {
                    id: project_id,
                    schema: self.project_state.project.deploy_schema.to_project_schema(),
                    creator_deposit,
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

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        let pool_address = static_pool_address(static_pool_mint.pubkey());

        let pool_mint_account =
            get_associated_token_address(&pool_address, &static_pool_mint.pubkey());

        let static_pool = self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("invariant: static pool config missing"))?;
        let finish_ts = static_pool.launch_ts;

        let ix = program
            .request()
            .accounts(moonzip::accounts::CreateStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                event_authority: *MOONZIP_EVENT_AUTHORITY,
                project: project_address,
                mint: static_pool_mint.pubkey(),
                pool_mint_account,
                pool: pool_address,

                program: moonzip::ID,
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

        let ix = program
            .request()
            .accounts(moonzip::accounts::GraduateStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                event_authority: *MOONZIP_EVENT_AUTHORITY,
                funds_receiver: moonzip::PROGRAM_AUTHORITY,
                project: project_address(&project_id(&self.project_state.project.id)),
                pool: self.static_pool_address()?,

                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: anchor_spl::token::ID,
            })
            .args(GraduateStaticPool {})
            .instructions()?;

        Ok(ix)
    }

    pub fn reward_creator_on_graduate(&self) -> anyhow::Result<Vec<Instruction>> {
        Ok(vec![system_instruction::transfer(
            &PROGRAM_AUTHORITY,
            &self.project_state.project.owner.to_pubkey(),
            self.config.creator_graduate_reward,
        )])
    }

    pub fn init_transmuter(&mut self) -> anyhow::Result<Vec<Instruction>> {
        if self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_none()
        {
            return Ok(vec![]);
        }
        let static_pool_mint = self
            .project_state
            .project
            .static_pool_mint()
            .ok_or_else(|| anyhow::anyhow!("invariant: static pool mint is not already stored"))?;
        let curve_mint = self.curve_mint()?;
        Ok(match self.project_state.project.deploy_schema.curve_pool {
            CurveVariant::Moonzip => self.add_transmuter_for_moonzip(TransmuterInitArgs {
                from_mint: static_pool_mint,
                to_mint: curve_mint,
                donor: PROGRAM_AUTHORITY,
            })?,
            CurveVariant::Pumpfun => self.add_transmuter_for_pumpfun(TransmuterInitArgs {
                from_mint: static_pool_mint,
                to_mint: curve_mint,
                donor: PROGRAM_AUTHORITY,
            })?,
        })
    }

    pub fn add_transmuter_for_moonzip(
        &mut self,
        args: TransmuterInitArgs,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let curved_pool = get_curved_pool_address(args.to_mint);
        let base = self.base_transmuter_init_accounts(&args);
        let ix = program
            .request()
            .accounts(moonzip::accounts::InitTransmuterForCurveAccounts { base, curved_pool })
            .args(moonzip::instruction::InitTransmuterForCurve {})
            .instructions()?;

        Ok(ix)
    }

    pub fn add_transmuter_for_pumpfun(
        &mut self,
        args: TransmuterInitArgs,
    ) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let bonding_curve = pumpfun::get_bonding_curve(&args.to_mint);

        let ix = program
            .request()
            .accounts(moonzip::accounts::InitTransmuterForPumpfunCurveAccounts {
                base: self.base_transmuter_init_accounts(&args),
                bonding_curve,
            })
            .args(moonzip::instruction::InitTransmuterForPumpfunCurve {})
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
            donor_to_mint_account: get_associated_token_address(&args.donor, &args.to_mint),
            donor: args.donor,
            transmuter_to_mint_account: get_associated_token_address(&transmuter, &args.to_mint),
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
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, &action.mint);
        let mut initial_curve =
            moonzip::pumpfun::CurveWrapper::initial(&pumpfun_meta.global_account);

        let mut buy = |user: Pubkey, sols: u64| {
            let buy_params = moonzip::pumpfun::BuyCalculator::from_curve_wrapper(&initial_curve)
                .fixed_sols(sols);
            initial_curve.commit_buy(buy_params.max_sol_cost, buy_params.tokens);

            Result::<_, anyhow::Error>::Ok(
                program
                    .request()
                    .accounts(pumpfun_cpi::accounts::Buy {
                        global: *pumpfun::GLOBAL,
                        event_authority: pumpfun::EVENT_AUTHORITY,
                        fee_recipient: pumpfun_meta.global_account.fee_recipient,

                        mint: action.mint,
                        bonding_curve,
                        associated_bonding_curve,

                        associated_user: get_associated_token_address(&user, &action.mint),
                        user,

                        system_program: solana_sdk::system_program::ID,
                        token_program: anchor_spl::token::ID,
                        rent: solana_sdk::sysvar::rent::ID,
                        program: pumpfun_cpi::ID,
                    })
                    .args(buy_params.as_ix_data())
                    .instructions()?,
            )
        };

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
                user: PROGRAM_AUTHORITY,
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

        if action.dev_purchase.is_some() || action.post_dev_purchase.is_some() {
            result.push(create_associated_token_account(
                &PROGRAM_AUTHORITY,
                &PROGRAM_AUTHORITY,
                &action.mint,
                &anchor_spl::token::ID,
            ));
        }
        if let Some(purchase) = action.dev_purchase {
            result.append(&mut buy(PROGRAM_AUTHORITY, purchase.sols)?);
        };
        if let Some(purchase) = action.post_dev_purchase {
            result.append(&mut buy(PROGRAM_AUTHORITY, purchase.sols)?);
        };
        result.append(&mut self.manual_project_graduate()?);

        Ok(result)
    }

    fn manual_project_graduate(&mut self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let project = self.get_project_address();

        let ix = program
            .request()
            .accounts(moonzip::accounts::GraduateProjectAccounts {
                event_authority: *MOONZIP_EVENT_AUTHORITY,
                authority: moonzip::PROGRAM_AUTHORITY,
                project,
                program: moonzip::ID,
            })
            .args(moonzip::instruction::ProjectGraduate {
                _data: moonzip::project::GraduateProjectData {
                    id: project_id(&self.project_state.project.id),
                },
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn init_moonzip_pool(&mut self, action: CurveCreate) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let pool_address = get_curved_pool_address(action.mint);
        let project = self.get_project_address();
        let pool_token_account = get_associated_token_address(&pool_address, &action.mint);

        let buy = |user: Pubkey, sols: u64| {
            Result::<_, anyhow::Error>::Ok(
                program
                    .request()
                    .accounts(moonzip::accounts::BuyFromCurvedPoolAccounts {
                        authority: moonzip::PROGRAM_AUTHORITY,
                        fee: fee_address(),
                        project,
                        mint: action.mint,

                        pool_token_account,
                        pool: pool_address,

                        user_token_account: get_associated_token_address(&user, &action.mint),
                        user,

                        event_authority: *MOONZIP_EVENT_AUTHORITY,
                        program: moonzip::ID,
                        system_program: solana_sdk::system_program::ID,
                        token_program: anchor_spl::token::ID,
                        associated_token_program: anchor_spl::associated_token::ID,
                    })
                    .args(moonzip::instruction::BuyFromCurvedPool {
                        data: BuyFromCurvedPoolData {
                            project_id: project_id(&self.project_state.project.id),
                            sols,
                            min_token_output: 0,
                        },
                    })
                    .instructions()?,
            )
        };

        let mut ix = program
            .request()
            .accounts(moonzip::accounts::CreateCurvedPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project,

                global: *mzip::GLOBAL_ACCOUNT,
                mint: action.mint,
                pool_token_account,
                pool: pool_address,

                event_authority: *MOONZIP_EVENT_AUTHORITY,
                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .args(moonzip::instruction::CreateCurvedPool {
                data: CreateCurvedPoolData {
                    project_id: project_id(&self.project_state.project.id),
                },
            })
            .instructions()?;

        if let Some(purchase) = action.dev_purchase {
            let sols = purchase.sols;
            ix.append(&mut buy(PROGRAM_AUTHORITY, sols)?);
        };

        if let Some(purchase) = action.post_dev_purchase {
            let sols = purchase.sols;
            ix.append(&mut buy(PROGRAM_AUTHORITY, sols)?)
        };

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
        if matches!(
            self.project_state.project.deploy_schema.curve_pool,
            CurveVariant::Pumpfun
        ) {
            bail!("pumpfun curve pools could not be graduated");
        }

        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;
        let curve_mint = self.curve_mint()?;
        let pool_address = curved_pool_address(curve_mint);
        let project_id = project_id(&self.project_state.project.id);

        let ix = program
            .request()
            .accounts(moonzip::accounts::GraduateCurvedPoolAccounts {
                event_authority: *MOONZIP_EVENT_AUTHORITY,

                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address(&project_id),
                fee: fee_address(),
                mint: curve_mint,

                funds_receiver: moonzip::PROGRAM_AUTHORITY,
                funds_receiver_ata: get_associated_token_address(
                    &moonzip::PROGRAM_AUTHORITY,
                    &curve_mint,
                ),

                pool: pool_address,
                pool_ata: get_associated_token_address(&pool_address, &curve_mint),

                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                token_program: anchor_spl::token::ID,
            })
            .args(moonzip::instruction::GraduateCurvedPool {
                _data: GraduateCurvedPoolData { project_id },
            })
            .instructions()?;

        Ok(ix)
    }

    pub fn lock_dev(&self) -> anyhow::Result<Vec<Instruction>> {
        self._lock_dev(self.dev_tokens_amount()?)
    }

    fn _lock_dev(&self, tokens: u64) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let sender = PROGRAM_AUTHORITY;
        let sender_ata = get_associated_token_address(&sender, &curve_mint);
        let owner = self.project_state.project.owner.to_pubkey();

        let Some(period) = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .map(|purchase| Duration::from_secs(purchase.lock_period as u64))
        else {
            bail!("invariant: dev purchase is not enabled for project")
        };
        if period.is_zero() {
            bail!("zero period must be delivered immediately, without locking");
        }

        let client = self.solana_pool.builder();
        let program_id = self.config.locker_program;

        let program = client.program(program_id)?;

        let base: Keypair = self
            .project_state
            .project
            .dev_lock_keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("invariant: no dev lock keypair provided"))?
            .to_keypair();
        let escrow_addr = lock::escrow_address(&base.pubkey(), &self.config.locker_program);
        let escrow_ata = get_associated_token_address(&escrow_addr, &curve_mint);

        let cliff_time = (TZ::now() + period).timestamp() as u64;
        tracing::debug!("would unlock {tokens} after {cliff_time}");

        let frequency = 1;

        let mut ixs = vec![create_associated_token_account(
            &sender,
            &escrow_addr,
            &curve_mint,
            &anchor_spl::token::ID,
        )];
        let mut create_vesting = program
            .request()
            .accounts(locker::accounts::CreateVestingEscrowV2 {
                base: base.pubkey(),
                escrow: escrow_addr,
                token_mint: curve_mint,
                escrow_token: escrow_ata,
                sender,
                sender_token: sender_ata,
                recipient: owner,
                event_authority: anchor_event_authority(&program_id),

                program: program_id,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
            })
            .args(locker::instruction::CreateVestingEscrowV2 {
                params: locker::CreateVestingEscrowParameters {
                    vesting_start_time: cliff_time,
                    cliff_time,
                    frequency,
                    cliff_unlock_amount: 0,
                    amount_per_period: tokens,
                    number_of_period: 1,
                    update_recipient_mode: 0,
                    cancel_mode: 0,
                },
                remaining_accounts_info: None,
            })
            .instructions()?;
        ixs.append(&mut create_vesting);
        Ok(ixs)
    }

    pub fn claim_dev_lock(&self) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let owner = self.project_state.project.owner.to_pubkey();

        let Some(period) = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .map(|purchase| Duration::from_secs(purchase.lock_period as u64))
        else {
            bail!("invariant: dev purchase is not enabled for project")
        };
        if period.is_zero() {
            bail!("zero period must be delivered immediately, without locking");
        }

        let client = self.solana_pool.builder();
        let program_id = self.config.locker_program;

        let program = client.program(program_id)?;

        let base: Keypair = self
            .project_state
            .project
            .dev_lock_keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("invariant: no dev lock keypair provided"))?
            .to_keypair();
        let escrow_addr = lock::escrow_address(&base.pubkey(), &self.config.locker_program);
        let escrow_ata = get_associated_token_address(&escrow_addr, &curve_mint);
        let owner_ata = get_associated_token_address(&owner, &curve_mint);

        let mut ixs = vec![create_associated_token_account_idempotent(
            &owner,
            &owner,
            &curve_mint,
            &anchor_spl::token::ID,
        )];
        let mut claim_ix = program
            .request()
            .accounts(locker::accounts::ClaimV2 {
                escrow: escrow_addr,
                token_mint: curve_mint,
                escrow_token: escrow_ata,
                recipient_token: owner_ata,
                recipient: owner,
                event_authority: anchor_event_authority(&program_id),

                memo_program: self.config.memo_program,
                program: program_id,
                token_program: anchor_spl::token::ID,
            })
            .args(locker::instruction::ClaimV2 {
                max_amount: u64::MAX,
                remaining_accounts_info: None,
            })
            .instructions()?;
        ixs.append(&mut claim_ix);
        Ok(ixs)
    }

    pub fn deliver_dev_tokens(&self) -> anyhow::Result<Vec<Instruction>> {
        let curve_mint = self.curve_mint()?;
        let sender = PROGRAM_AUTHORITY;
        let sender_ata = get_associated_token_address(&sender, &curve_mint);
        let owner = self.project_state.project.owner.to_pubkey();
        let owner_ata = get_associated_token_address(&owner, &curve_mint);

        let tokens = self.dev_tokens_amount()?;
        tracing::debug!("would deliver {tokens} to dev {owner}");

        Ok(vec![
            create_associated_token_account(&sender, &owner, &curve_mint, &anchor_spl::token::ID),
            spl_token::instruction::transfer(
                &anchor_spl::token::ID,
                &sender_ata,
                &owner_ata,
                &sender,
                &[],
                tokens,
            )?,
        ])
    }

    fn dev_tokens_amount(&self) -> anyhow::Result<u64> {
        let dev_purchase = self
            .project_state
            .project
            .deploy_schema
            .dev_purchase
            .as_ref()
            .ok_or_else(|| {
                anyhow::anyhow!("invariant: dev purchase is missing for dev delivery")
            })?;
        let sols = u64::try_from(dev_purchase.amount.to_owned())?;
        let tokens = match self.project_state.project.deploy_schema.curve_pool {
            CurveVariant::Pumpfun => {
                let initial = moonzip::pumpfun::CurveWrapper::initial(
                    &self.pump_meta.clone().get()?.global_account,
                );
                moonzip::pumpfun::BuyCalculator::from_curve_wrapper(&initial)
                    .fixed_sols(sols)
                    .tokens
            }
            CurveVariant::Moonzip => {
                let meta = self.mzip_meta.clone().get()?;
                let initial = moonzip::curved_pool::curve::CurveState::from_cfg(
                    &meta.global_account.config.curve,
                );
                let result = moonzip::curved_pool::curve::BuyCalculator::new(&initial)
                    .with_fee(meta.fee_account.config.on_buy)
                    .fixed_sols(sols);
                result
            }
        };
        Ok(tokens)
    }

    pub fn lock_project(&self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let ix = program
            .request()
            .accounts(moonzip::accounts::ProjectLockLatchAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project: self.get_project_address(),
            })
            .args(moonzip::instruction::ProjectLockLatch {})
            .instructions()?;

        Ok(ix)
    }

    pub fn unlock_project(&self) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        let ix = program
            .request()
            .accounts(moonzip::accounts::ProjectUnlockLatchAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address,
            })
            .args(moonzip::instruction::ProjectUnlockLatch {})
            .instructions()?;

        Ok(ix)
    }

    pub fn buy(
        &mut self,
        user: Pubkey,
        sols: u64,
        min_token_output: Option<u64>,
    ) -> anyhow::Result<Vec<Instruction>> {
        let mut ixs = vec![];

        let has_static_pool = self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_some();
        let buy_params = BuyParams {
            user,
            sols,
            min_token_output: min_token_output.unwrap_or(0),
        };
        match self.project_state.project.stage {
            Stage::OnStaticPool => ixs.append(&mut self.buy_from_static_pool(buy_params)?),
            Stage::OnCurvePool => {
                if has_static_pool {
                    ixs.append(&mut self.transmute_idempotent(user)?);
                }
                ixs.append(&mut self.buy_from_curve_pool(buy_params)?);
            }
            Stage::Graduated => {
                if has_static_pool {
                    ixs.append(&mut self.transmute_idempotent(user)?);
                }
                if self.project_state.project.deploy_schema.curve_pool == CurveVariant::Pumpfun {
                    ixs.append(&mut self.buy_from_pumpfun(buy_params)?);
                } else {
                    ixs.append(&mut self.buy_from_raydium(buy_params)?);
                }
            }
            _ => bail!(
                "unable to buy from project: stage mismatch: {:?}",
                self.project_state.project.stage
            ),
        };

        Ok(ixs)
    }

    fn buy_from_static_pool(&self, params: BuyParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let static_pool_mint = self
            .project_state
            .project
            .static_pool_mint()
            .ok_or_else(|| anyhow::anyhow!("invariant: no static pool mint"))?;
        let pool = static_pool_address(static_pool_mint);

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        Ok(program
            .request()
            .accounts(moonzip::accounts::BuyFromStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,

                fee: *FEE_ACCOUNT,
                project: project_address,
                user: params.user,
                mint: static_pool_mint,
                user_mint_account: get_associated_token_address(&params.user, &static_pool_mint),
                pool_mint_account: get_associated_token_address(&pool, &static_pool_mint),
                pool,

                event_authority: *MOONZIP_EVENT_AUTHORITY,
                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .args(moonzip::instruction::BuyFromStaticPool {
                data: BuyFromStaticPoolData {
                    project_id,
                    sols: params.sols,
                },
            })
            .instructions()?)
    }

    fn buy_from_curve_pool(&self, params: BuyParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let curve_mint = self.curve_mint()?;
        let curve_pool = get_curved_pool_address(curve_mint);

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        Ok(program
            .request()
            .accounts(moonzip::accounts::BuyFromCurvedPoolAccounts {
                event_authority: *MOONZIP_EVENT_AUTHORITY,
                authority: moonzip::PROGRAM_AUTHORITY,
                project: project_address,
                fee: fee_address(),
                user: params.user,
                mint: curve_mint,
                user_token_account: get_associated_token_address(&params.user, &curve_mint),
                pool_token_account: get_associated_token_address(&curve_pool, &curve_mint),
                pool: curve_pool,

                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .args(moonzip::instruction::BuyFromCurvedPool {
                data: BuyFromCurvedPoolData {
                    project_id,
                    sols: params.sols,
                    min_token_output: params.min_token_output,
                },
            })
            .instructions()?)
    }

    fn buy_from_pumpfun(&mut self, params: BuyParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let curve_mint = self.curve_mint()?;
        let pumpfun_meta = self.pump_meta.get()?;
        let bonding_curve = pumpfun::get_bonding_curve(&curve_mint);
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, &curve_mint);

        let ixs = program
            .request()
            .accounts(moonzip::accounts::BuyFromPumpAccounts {
                global: *pumpfun::GLOBAL,
                event_authority: pumpfun::EVENT_AUTHORITY,
                fee_recipient: pumpfun_meta.global_account.fee_recipient,

                mint: curve_mint,
                bonding_curve,
                associated_bonding_curve,

                associated_user: get_associated_token_address(&params.user, &curve_mint),
                user: params.user,

                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                rent: solana_sdk::sysvar::rent::ID,
                program: pumpfun_cpi::ID,
                associated_token_program: associated_token::ID,
            })
            .args(BuyFromPump {
                data: BuyFromPumpData {
                    sols: params.sols,
                    min_token_output: params.min_token_output,
                },
            })
            .instructions()?;
        Ok(ixs)
    }

    pub fn sell(
        &mut self,
        user: Pubkey,
        tokens: u64,
        min_sol_output: Option<u64>,
    ) -> anyhow::Result<Vec<Instruction>> {
        let has_static_pool = self
            .project_state
            .project
            .deploy_schema
            .static_pool
            .is_some();

        let mut ixs = vec![];
        let sell_params = SellParams {
            user,
            tokens,
            min_sol_output: min_sol_output.unwrap_or_default(),
        };

        match self.project_state.project.stage {
            Stage::OnStaticPool => {
                ixs.append(&mut self.sell_to_static_pool(sell_params)?);
            }
            Stage::OnCurvePool => {
                if has_static_pool {
                    ixs.append(&mut self.transmute_idempotent(user)?);
                }
                ixs.append(&mut self.sell_to_curve_pool(sell_params)?);
            }
            Stage::Graduated => {
                if has_static_pool {
                    ixs.append(&mut self.transmute_idempotent(user)?);
                }
                if self.project_state.project.deploy_schema.curve_pool == CurveVariant::Pumpfun {
                    ixs.append(&mut self.sell_to_pumpfun(sell_params)?);
                } else {
                    ixs.append(&mut self.sell_to_raydium(sell_params)?);
                }
            }
            _ => {
                bail!(
                    "unable to sell to project: stage mismatch: {:?}",
                    self.project_state.project.stage
                );
            }
        };

        Ok(ixs)
    }

    fn sell_to_static_pool(&self, params: SellParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        let static_pool_mint = self
            .project_state
            .project
            .static_pool_mint()
            .ok_or_else(|| anyhow::anyhow!("invariant: no static pool mint"))?;
        let pool = static_pool_address(static_pool_mint);
        Ok(program
            .request()
            .accounts(moonzip::accounts::SellToStaticPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,

                fee: *FEE_ACCOUNT,
                project: project_address,
                user: params.user,
                mint: static_pool_mint,
                user_token_account: get_associated_token_address(&params.user, &static_pool_mint),
                pool_token_account: get_associated_token_address(&pool, &static_pool_mint),
                pool,

                event_authority: *MOONZIP_EVENT_AUTHORITY,
                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .args(moonzip::instruction::SellToStaticPool {
                data: SellToStaticPoolData {
                    project_id,
                    tokens: params.tokens,
                },
            })
            .instructions()?)
    }

    fn sell_to_curve_pool(&self, params: SellParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let project_id = project_id(&self.project_state.project.id);
        let project_address = project_address(&project_id);

        let curve_mint = self.curve_mint()?;
        let curve_pool = get_curved_pool_address(curve_mint);

        Ok(program
            .request()
            .accounts(moonzip::accounts::SellFromCurvedPoolAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,

                fee: fee_address(),
                project: project_address,
                user: params.user,
                mint: curve_mint,
                user_token_account: get_associated_token_address(&params.user, &curve_mint),
                pool_token_account: get_associated_token_address(&curve_pool, &curve_mint),
                pool: curve_pool,

                event_authority: *MOONZIP_EVENT_AUTHORITY,
                program: moonzip::ID,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
            })
            .args(moonzip::instruction::SellFromCurvedPool {
                data: SellFromCurvedPoolData {
                    project_id,
                    tokens: params.tokens,
                    min_sol_output: params.min_sol_output,
                },
            })
            .instructions()?)
    }

    fn sell_to_pumpfun(&mut self, params: SellParams) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(pumpfun_cpi::ID)?;

        let curve_mint = self.curve_mint()?;
        let pumpfun_meta = self.pump_meta.get()?;
        let bonding_curve = pumpfun::get_bonding_curve(&curve_mint);
        let associated_bonding_curve = get_associated_token_address(&bonding_curve, &curve_mint);

        let ixs = program
            .request()
            .accounts(pumpfun_cpi::accounts::Sell {
                global: *pumpfun::GLOBAL,
                event_authority: pumpfun::EVENT_AUTHORITY,
                fee_recipient: pumpfun_meta.global_account.fee_recipient,

                mint: curve_mint,
                bonding_curve,
                associated_bonding_curve,

                associated_user: get_associated_token_address(&params.user, &curve_mint),
                user: params.user,

                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                program: pumpfun_cpi::ID,
                associated_token_program: associated_token::ID,
            })
            .args(pumpfun_cpi::instruction::Sell {
                _amount: params.tokens,
                _min_sol_output: params.min_sol_output,
            })
            .instructions()?;
        Ok(ixs)
    }

    fn transmute_idempotent(&self, user: Pubkey) -> anyhow::Result<Vec<Instruction>> {
        let client = self.solana_pool.builder();
        let program = client.program(moonzip::ID)?;

        let curve_mint = self.curve_mint()?;
        let static_pool_mint = self.static_pool_mint()?;
        let transmuter = get_transmuter_address(static_pool_mint, curve_mint);

        Ok(program
            .request()
            .accounts(moonzip::accounts::TransmuteIdempotentAccounts {
                authority: moonzip::PROGRAM_AUTHORITY,
                user,
                from_mint: static_pool_mint,
                to_mint: curve_mint,
                user_from_token_account: get_associated_token_address(&user, &static_pool_mint),
                user_to_token_account: get_associated_token_address(&user, &curve_mint),
                transmuter_to_token_account: get_associated_token_address(&transmuter, &curve_mint),
                transmuter,
                system_program: solana_sdk::system_program::ID,
                token_program: anchor_spl::token::ID,
                associated_token_program: anchor_spl::associated_token::ID,
                moonzip_program: moonzip::ID,
            })
            .args(moonzip::instruction::TransmuteIdempotent {})
            .instructions()?)
    }

    fn get_project_address(&self) -> Pubkey {
        project_address(&project_id(&self.project_state.project.id))
    }

    fn static_pool_address(&self) -> anyhow::Result<Pubkey> {
        Ok(static_pool_address(
            self.project_state
                .project
                .static_pool_mint()
                .ok_or_else(|| anyhow::anyhow!("no static pool mint stored"))?,
        ))
    }

    fn curve_mint(&self) -> anyhow::Result<Pubkey> {
        Ok(self
            .project_state
            .project
            .curve_pool_keypair
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("invariant: no curve mint"))?
            .to_keypair()
            .pubkey())
    }

    fn static_pool_mint(&self) -> anyhow::Result<Pubkey> {
        let static_pool_mint = self
            .project_state
            .project
            .static_pool_mint()
            .ok_or_else(|| anyhow::anyhow!("invariant: no static pool mint"))?;
        Ok(static_pool_mint)
    }

    /// Seed is needed for deriving pubkeys.
    fn project_seed(&self) -> String {
        self.project_state.project.id.simple().to_string()
    }

    /// It allows generating unique solana-conforming seeds,
    /// which are practically unique as long as combination (project_id, extra_seed) is unique.
    ///
    /// Lets say you want to generate market account that is bound to specific project.
    /// You decide that seed would be "openbook_market" - it would be extra_seed parameter.
    /// Then resulting String is unique across all projects.
    fn project_derived_seed(&self, extra_seed: &str) -> String {
        let mut hasher = Sha256::new();
        hasher.update(format!("{}__{extra_seed}", self.project_seed()).into_bytes());
        let mut result = format!("{:x}", hasher.finalize());
        result.truncate(32);
        result
    }
}

#[derive(Debug, Clone)]
pub struct BuyParams {
    user: Pubkey,
    sols: u64,
    min_token_output: u64,
}

#[derive(Debug, Clone)]
pub struct SellParams {
    user: Pubkey,
    tokens: u64,
    min_sol_output: u64,
}

#[derive(Debug, Clone)]
pub struct CurveCreate {
    pub mint: Pubkey,
    pub dev_purchase: Option<InitialPurchase>,
    pub post_dev_purchase: Option<InitialPurchase>,
    pub metadata: StoredTokenMeta,
}

#[derive(Debug, Clone)]
pub struct InitialPurchase {
    pub user: Pubkey,
    pub sols: u64,
}

pub struct TransmuterInitArgs {
    pub from_mint: Pubkey,
    pub to_mint: Pubkey,
    pub donor: Pubkey,
}

#[derive(Debug, Clone)]
pub struct SeedDerivedPubkey {
    pub key: Pubkey,
    pub seed: String,
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
