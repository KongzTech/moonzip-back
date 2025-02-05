use crate::{ensure_account_size, utils::Sizable, PROGRAM_AUTHORITY};
use anchor_lang::{prelude::*, system_program};
use derive_more::derive::{From, Into};

pub const PROJECT_PREFIX: &[u8] = b"project";

pub fn project_address(id: &ProjectId) -> Pubkey {
    let (address, _) = Pubkey::find_program_address(&[b"project", &id.to_bytes()], &crate::ID);
    address
}

pub fn create(ctx: Context<CreateProjectAccounts>, data: CreateProjectData) -> Result<()> {
    ctx.accounts.project.set_inner(Project {
        id: data.id,
        schema: data.schema,
        stage: ProjectStage::Created,
        latch: ProjectLatch::new(data.creator_deposit),
        bump: ctx.bumps.project,
    });

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.creator.to_account_info(),
                to: ctx.accounts.authority.to_account_info(),
            },
        ),
        data.creator_deposit,
    )?;

    Ok(())
}

pub fn lock_latch(ctx: Context<ProjectLockLatchAccounts>) -> Result<()> {
    ctx.accounts.project.latch.lock(&ctx.accounts.authority)?;
    Ok(())
}

pub fn unlock_latch(ctx: Context<ProjectUnlockLatchAccounts>) -> Result<()> {
    ctx.accounts.project.latch.unlock(&ctx.accounts.authority)?;
    Ok(())
}

#[derive(
    AnchorSerialize,
    AnchorDeserialize,
    Clone,
    Debug,
    Into,
    From,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Default,
)]
pub struct ProjectId(pub u128);

impl ProjectId {
    pub fn to_bytes(&self) -> [u8; 16] {
        self.0.to_le_bytes()
    }
}

impl Sizable for ProjectId {
    fn longest() -> Self {
        ProjectId(Sizable::longest())
    }
}

#[account]
#[derive(Debug)]
pub struct Project {
    pub id: ProjectId,
    pub schema: ProjectSchema,
    pub stage: ProjectStage,
    pub latch: ProjectLatch,
    pub bump: u8,
}

impl Project {
    pub fn ensure_can_create_static_pool(&self) -> Result<()> {
        if !self.schema.use_static_pool {
            return err!(ProjectError::SchemaMismatch);
        }
        if self.stage != ProjectStage::Created {
            return err!(ProjectError::StaticPoolAlreadyExists);
        }
        Ok(())
    }

    pub fn ensure_can_create_curved_pool(&self) -> Result<()> {
        if self.schema.use_static_pool {
            if !matches!(self.stage, ProjectStage::StaticPoolClosed { .. }) {
                return err!(ProjectError::StaticPoolNotClosed);
            }
        } else if self.stage != ProjectStage::Created {
            return err!(ProjectError::CurvePoolAlreadyExists);
        }
        Ok(())
    }
}

impl Sizable for Project {
    fn longest() -> Self {
        Project {
            id: Sizable::longest(),
            schema: ProjectSchema::longest(),
            stage: ProjectStage::Created,
            latch: ProjectLatch::longest(),
            bump: Sizable::longest(),
        }
    }
}

ensure_account_size!(Project, 54);

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum ProjectStage {
    Created,

    StaticPoolActive,
    StaticPoolClosed,

    CurvePoolActive,
    CurvePoolClosed,

    Graduated,
}

impl Sizable for ProjectStage {
    fn longest() -> Self {
        Self::Created
    }
}

#[derive(Accounts)]
#[instruction(data: CreateProjectData)]
pub struct CreateProjectAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub creator: Signer<'info>,

    #[account(
        init,
        payer = creator,
        space = Project::ACCOUNT_SIZE,
        seeds = [PROJECT_PREFIX, &data.id.to_bytes()], bump,
    )]
    pub project: Account<'info, Project>,

    pub system_program: Program<'info, System>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct CreateProjectData {
    pub id: ProjectId,
    pub schema: ProjectSchema,
    pub creator_deposit: u64,
}

#[derive(Accounts)]
pub struct ProjectLockLatchAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub project: Account<'info, Project>,
}

#[derive(Accounts)]
pub struct ProjectUnlockLatchAccounts<'info> {
    #[account(mut, constraint = authority.key == &PROGRAM_AUTHORITY)]
    pub authority: Signer<'info>,

    #[account(mut)]
    pub project: Account<'info, Project>,
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ProjectSchema {
    pub use_static_pool: bool,
    pub curve_pool: CurvePoolVariant,
    pub dev_purchase: Option<u64>,
}

impl Sizable for ProjectSchema {
    fn longest() -> Self {
        ProjectSchema {
            use_static_pool: false,
            curve_pool: CurvePoolVariant::longest(),
            dev_purchase: Some(Sizable::longest()),
        }
    }
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub enum CurvePoolVariant {
    Moonzip,
    Pumpfun,
}

impl Sizable for CurvePoolVariant {
    fn longest() -> Self {
        CurvePoolVariant::Moonzip
    }
}

#[derive(AnchorDeserialize, AnchorSerialize, Clone, Debug)]
pub struct ProjectLatch {
    pub project_bank: u64,
    pub lamports_before_tx: Option<u64>,
}

impl ProjectLatch {
    pub fn new(bank: u64) -> Self {
        Self {
            project_bank: bank,
            lamports_before_tx: None,
        }
    }

    pub fn lock(&mut self, authority: &Signer) -> Result<()> {
        if self.lamports_before_tx.is_some() {
            return err!(ProjectError::ProjectLatchAlreadyLocked);
        }
        self.lamports_before_tx = Some(authority.lamports());
        Ok(())
    }

    pub fn unlock(&mut self, authority: &Signer) -> Result<()> {
        let Some(before) = self.lamports_before_tx.take() else {
            return err!(ProjectError::ProjectLatchNotLocked);
        };
        let after = authority.lamports();

        if before > after {
            let diff = before - after;
            if self.project_bank.checked_sub(diff).is_none() {
                msg!(
                    "Bank overuse, need {} lamports, left: {}",
                    diff,
                    self.project_bank
                );
                return err!(ProjectError::BankOveruse);
            }
        } else {
            self.project_bank += after - before;
        }

        Ok(())
    }
}

impl Sizable for ProjectLatch {
    fn longest() -> Self {
        ProjectLatch {
            project_bank: Sizable::longest(),
            lamports_before_tx: Some(Sizable::longest()),
        }
    }
}

#[error_code]
pub enum ProjectError {
    #[msg("Project schema conflicts with instruction")]
    SchemaMismatch,

    #[msg("Static pool already exists")]
    StaticPoolAlreadyExists,

    #[msg("Static pool is not closed yet")]
    StaticPoolNotClosed,

    #[msg("Curve pool already exists")]
    CurvePoolAlreadyExists,

    #[msg("Project is already graduated")]
    AlreadyGraduated,

    #[msg("Project latch is already locked")]
    ProjectLatchAlreadyLocked,

    #[msg("Project latch isn't locked: must be done in the start")]
    ProjectLatchNotLocked,

    #[msg("Project bank is overused")]
    BankOveruse,
}
