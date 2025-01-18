use crate::{
    curved_pool::CurvedPool, ensure_account_size, static_pool::StaticPool, utils::Sizable,
    PROGRAM_AUTHORITY,
};
use anchor_lang::{prelude::*, solana_program::native_token::LAMPORTS_PER_SOL, system_program};
use derive_more::derive::{From, Into};

pub const PROJECT_PREFIX: &[u8] = b"project";
const PUMPFUN_INIT_PRICE: u64 = (0.022 * LAMPORTS_PER_SOL as f64) as u64;

pub fn create(ctx: Context<CreateProjectAccounts>, data: CreateProjectData) -> Result<()> {
    let mut amount = 0;
    if data.schema.use_static_pool {
        amount += Rent::get()?.minimum_balance(StaticPool::ACCOUNT_SIZE);
    }
    match data.schema.curve_pool {
        CurvePoolVariant::Moonzip => {
            amount += Rent::get()?.minimum_balance(CurvedPool::ACCOUNT_SIZE);
        }
        CurvePoolVariant::Pumpfun => {
            amount += PUMPFUN_INIT_PRICE;
        }
    }

    ctx.accounts.project.set_inner(Project {
        id: data.id,
        schema: data.schema,
        bump: ctx.bumps.project,
    });

    system_program::transfer(
        CpiContext::new(
            ctx.accounts.system_program.to_account_info(),
            system_program::Transfer {
                from: ctx.accounts.creator.to_account_info(),
                to: ctx.accounts.project.to_account_info(),
            },
        ),
        amount,
    )?;

    Ok(())
}

#[derive(
    AnchorSerialize, AnchorDeserialize, Clone, Debug, Into, From, PartialEq, Eq, PartialOrd, Ord,
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
    pub bump: u8,
}

impl Sizable for Project {
    fn longest() -> Self {
        Project {
            id: Sizable::longest(),
            schema: ProjectSchema::longest(),
            bump: Sizable::longest(),
        }
    }
}

ensure_account_size!(Project, 27);

#[derive(Accounts)]
#[instruction(data: CreateProjectData)]
pub struct CreateProjectAccounts<'info> {
    #[account(constraint = authority.key == &PROGRAM_AUTHORITY)]
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
}

#[derive(AnchorSerialize, AnchorDeserialize, Clone, Debug)]
pub struct ProjectSchema {
    pub use_static_pool: bool,
    pub curve_pool: CurvePoolVariant,
}

impl Sizable for ProjectSchema {
    fn longest() -> Self {
        ProjectSchema {
            use_static_pool: false,
            curve_pool: CurvePoolVariant::longest(),
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
