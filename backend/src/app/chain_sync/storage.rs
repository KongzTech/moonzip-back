use std::ops::DerefMut as _;

use moonzip::events::{ProjectChangedEvent, StaticPoolBuyEvent, StaticPoolSellEvent};
use tokio::{spawn, sync::mpsc::Receiver, task::JoinHandle};
use tracing::{debug, error};

use crate::app::{
    chain_sync::parser::MoonzipEvent,
    storage::{
        misc::Balance,
        project::{self, from_chain_project_id},
        DBTransaction, StorageClient,
    },
};

use super::parser::{ParseResult, TrackedEvent};

pub struct StorageApplier {
    storage_client: StorageClient,
    parsed_rx: Receiver<ParseResult>,
}

impl StorageApplier {
    pub fn new(storage_client: StorageClient, parse_results: Receiver<ParseResult>) -> Self {
        Self {
            storage_client,
            parsed_rx: parse_results,
        }
    }

    pub fn serve(mut self) -> JoinHandle<()> {
        spawn(async move {
            loop {
                if let Err(err) = self.tick().await {
                    error!("storage applier tick error: {err:#}")
                }
            }
        })
    }

    async fn tick(&mut self) -> anyhow::Result<()> {
        while let Some(result) = self.parsed_rx.recv().await {
            let mut tx = TransactionProcessor::new(
                self.storage_client.serializable_tx().await?,
                result.slot_number,
            );
            for event in result.events {
                tx.process_event(event).await?;
            }
            tx.commit().await?
        }
        anyhow::bail!("unexpected disconnect from parser")
    }
}

struct TransactionProcessor<'a> {
    transaction: DBTransaction<'a>,
    slot_number: u64,
}

impl<'a> TransactionProcessor<'a> {
    fn new(tx: DBTransaction<'a>, slot_number: u64) -> Self {
        Self {
            transaction: tx,
            slot_number,
        }
    }

    async fn process_event(&mut self, event: TrackedEvent) -> anyhow::Result<()> {
        tracing::trace!("applying event {event:?} on slot {}", self.slot_number);
        match event {
            super::parser::TrackedEvent::Moonzip(event) => match event {
                MoonzipEvent::ProjectChanged(project_changed) => {
                    apply_project_changed(&mut self.transaction, &project_changed).await?;
                }
                MoonzipEvent::StaticPoolBuy(event) => {
                    apply_static_pool_buy(&mut self.transaction, &event).await?;
                }
                MoonzipEvent::StaticPoolSell(event) => {
                    apply_static_pool_sell(&mut self.transaction, &event).await?;
                }
                _ => {
                    error!("some mzip tracked event not implemented")
                }
            },
            _ => {
                error!("some tracked event not implemented")
            }
        }

        Ok(())
    }

    async fn commit(self) -> anyhow::Result<()> {
        debug!("commit transaction for slot {}", self.slot_number);
        self.transaction.commit().await?;
        Ok(())
    }
}

async fn apply_project_changed(
    tx: &mut DBTransaction<'_>,
    event: &ProjectChangedEvent,
) -> anyhow::Result<()> {
    let stored = project::Stage::from_chain(event.to_stage);
    let project_id = from_chain_project_id(event.project_id);

    sqlx::query!(
        "
                UPDATE project
                SET stage = $2
                WHERE project.id = $1;
        ",
        &project_id,
        &stored as _
    )
    .execute(tx.deref_mut())
    .await?;
    Ok(())
}

async fn apply_static_pool_buy(
    tx: &mut DBTransaction<'_>,
    event: &StaticPoolBuyEvent,
) -> anyhow::Result<()> {
    let project_id = from_chain_project_id(event.project_id);
    let collected_lamports = Balance::from(event.new_collected_sols);

    sqlx::query!(
        "
                UPDATE static_pool_chain_state
                SET collected_lamports = $2
                WHERE project_id = $1;
        ",
        &project_id,
        &collected_lamports as _
    )
    .execute(tx.deref_mut())
    .await?;

    Ok(())
}

async fn apply_static_pool_sell(
    tx: &mut DBTransaction<'_>,
    event: &StaticPoolSellEvent,
) -> anyhow::Result<()> {
    let project_id = from_chain_project_id(event.project_id);
    let collected_lamports = Balance::from(event.new_collected_sols);

    sqlx::query!(
        "
                UPDATE static_pool_chain_state
                SET collected_lamports = $2
                WHERE project_id = $1;
        ",
        &project_id,
        &collected_lamports as _
    )
    .execute(tx.deref_mut())
    .await?;

    Ok(())
}
