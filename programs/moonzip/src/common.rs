use crate::utils::Sizable;
use anchor_lang::prelude::*;
use anchor_lang::{
    err,
    prelude::{Clock, SolanaSysvar},
    AnchorDeserialize, AnchorSerialize,
};
use derive_more::derive::{From, Into};

const ALLOWED_TIME_DRIFT_SECONDS: u64 = 1;

#[derive(AnchorSerialize, AnchorDeserialize, Default, Clone, PartialEq, PartialOrd)]
pub struct PoolCloseConditions {
    pub max_lamports: Option<u64>,
    pub finish_ts: Option<u64>,
}

impl Sizable for PoolCloseConditions {
    fn longest() -> Self {
        Self {
            max_lamports: Some(Sizable::longest()),
            finish_ts: Some(Sizable::longest()),
        }
    }
}

impl PoolCloseConditions {
    pub fn should_be_closed(&self, balance: u64, current_ts: u64) -> bool {
        let mut is_closed = false;
        if let Some(max_lamports) = self.max_lamports {
            is_closed = is_closed || (balance == max_lamports);
        }
        if let Some(finish_ts) = self.finish_ts {
            is_closed = is_closed || (current_ts >= finish_ts - ALLOWED_TIME_DRIFT_SECONDS)
        }
        is_closed
    }

    pub fn validate(&self) -> Result<()> {
        if let Some(finish_ts) = self.finish_ts {
            if finish_ts < (Clock::get()?.unix_timestamp as u64) {
                return err!(CommonUtilsError::CloseDateBehindClock);
            }
        }

        if self.finish_ts.is_none() && self.max_lamports.is_none() {
            return err!(CommonUtilsError::BoundaryConditionsNotSet);
        }

        Ok(())
    }
}

#[derive(
    AnchorDeserialize,
    AnchorSerialize,
    Default,
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    From,
    Into,
)]
pub struct PercentageBP(u16);

impl PercentageBP {
    pub fn apply_to(&self, value: u64) -> u64 {
        value * (self.0 as u64) / 10000
    }
}

#[error_code]
pub enum CommonUtilsError {
    #[msg("If close date is specified, it must be of the future")]
    CloseDateBehindClock,

    #[msg("Boundary conditions must be set for the pool - either close date, or max lamports, or both..")]
    BoundaryConditionsNotSet,
}
