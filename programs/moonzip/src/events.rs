use anchor_lang::prelude::*;

use crate::{ProjectId, ProjectStage};

#[must_use]
#[event]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct ProjectChangedEvent {
    pub project_id: ProjectId,

    pub from_stage: ProjectStage,
    pub to_stage: ProjectStage,
}

#[must_use]
#[event]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CurvedPoolBuyEvent {
    pub project_id: ProjectId,
    pub user: Pubkey,

    pub request_sols: u64,
    pub min_token_output: u64,
    pub tokens_output: u64,

    pub new_virtual_token_reserves: u64,
    pub new_virtual_sol_reserves: u64,
}

impl CurvedPoolBuyEvent {
    pub fn taken_fee(&self) -> u64 {
        self.request_sols - (self.new_virtual_sol_reserves - self.old_virtual_sol_reserves())
    }

    pub fn old_virtual_token_reserves(&self) -> u64 {
        self.new_virtual_token_reserves - self.tokens_output
    }

    pub fn old_virtual_sol_reserves(&self) -> u64 {
        (self.constant() / (self.old_virtual_token_reserves() as u128)) as u64
    }

    pub fn constant(&self) -> u128 {
        (self.new_virtual_token_reserves as u128) * (self.new_virtual_sol_reserves as u128)
    }
}

#[must_use]
#[event]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct CurvedPoolSellEvent {
    pub project_id: ProjectId,
    pub user: Pubkey,

    pub request_tokens: u64,
    pub min_sol_output: u64,
    pub sols_output: u64,

    pub new_virtual_token_reserves: u64,
    pub new_virtual_sol_reserves: u64,
}

impl CurvedPoolSellEvent {
    pub fn taken_fee(&self) -> u64 {
        self.new_virtual_sol_reserves - self.old_virtual_sol_reserves() - self.sols_output
    }

    pub fn old_virtual_token_reserves(&self) -> u64 {
        self.new_virtual_token_reserves - self.request_tokens
    }

    pub fn old_virtual_sol_reserves(&self) -> u64 {
        (self.constant() / (self.old_virtual_token_reserves() as u128)) as u64
    }

    pub fn constant(&self) -> u128 {
        (self.new_virtual_token_reserves as u128) * (self.new_virtual_sol_reserves as u128)
    }
}

#[must_use]
#[event]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct StaticPoolBuyEvent {
    pub project_id: ProjectId,
    pub user: Pubkey,

    pub request_sols: u64,
    pub output_tokens: u64,
    pub new_collected_sols: u64,
}

impl StaticPoolBuyEvent {
    pub fn taken_fee(&self) -> u64 {
        self.request_sols - self.output_tokens
    }
}

#[must_use]
#[event]
#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct StaticPoolSellEvent {
    pub project_id: ProjectId,
    pub user: Pubkey,

    pub request_tokens: u64,
    pub output_sols: u64,
    pub new_collected_sols: u64,
}

impl StaticPoolSellEvent {
    pub fn taken_fee(&self) -> u64 {
        self.request_tokens - self.output_sols
    }
}
