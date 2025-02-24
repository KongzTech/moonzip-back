use std::sync::Arc;

use anchor_client::anchor_lang::{
    prelude::event::EVENT_IX_TAG_LE, AnchorDeserialize, Discriminator,
};
use anyhow::{bail, Context as _};
use moonzip::events::{
    CurvedPoolBuyEvent, CurvedPoolSellEvent, ProjectChangedEvent, StaticPoolBuyEvent,
    StaticPoolSellEvent,
};
use rayon::iter::{IntoParallelIterator, ParallelIterator};
use solana_sdk::pubkey::Pubkey;
use tokio::{
    spawn,
    sync::mpsc::{channel, Receiver, Sender},
    task::spawn_blocking,
};
use tracing::{debug, error, instrument};
use yellowstone_grpc_proto::prelude::{
    InnerInstruction, InnerInstructions, Transaction, TransactionStatusMeta,
};

use crate::{define_discriminator, utils::ANCHOR_DISCRIMINATOR_BYTE_SIZE};

use super::cfg::ChainSyncConfig;

const BUFFER_CAPACITY: usize = 1000;
const PROJECT_CHANGED_EVENT: &[u8] = ProjectChangedEvent::DISCRIMINATOR.as_slice();

const CURVE_POOL_BUY_EVENT: &[u8] = CurvedPoolBuyEvent::DISCRIMINATOR.as_slice();
const CURVE_POOL_SELL_EVENT: &[u8] = CurvedPoolSellEvent::DISCRIMINATOR.as_slice();

const STATIC_POOL_SELL_EVENT: &[u8] = StaticPoolSellEvent::DISCRIMINATOR.as_slice();
const STATIC_POOL_BUY_EVENT: &[u8] = StaticPoolBuyEvent::DISCRIMINATOR.as_slice();
const TRACKED_PROGRAMS: &[Pubkey] = &[moonzip::ID_CONST, pumpfun_cpi::ID_CONST];

define_discriminator!(TradeEvent, &[189, 219, 127, 211, 78, 230, 97, 238]);

pub struct ParseInput {
    pub slot: u64,
    pub transaction: Transaction,
    pub meta: TransactionStatusMeta,
}

pub struct ParseResult {
    pub slot_number: u64,
    pub events: Vec<TrackedEvent>,
}

pub struct ParseAggregator {
    input_receiver: Receiver<ParseInput>,
    results_sender: Option<Sender<ParseResult>>,
    config: Arc<ChainSyncConfig>,
}

impl ParseAggregator {
    pub fn new(blocks: Receiver<ParseInput>, cfg: ChainSyncConfig) -> Self {
        Self {
            input_receiver: blocks,
            results_sender: None,
            config: Arc::new(cfg),
        }
    }

    pub fn serve(mut self) -> Receiver<ParseResult> {
        let (tx, rx) = channel(BUFFER_CAPACITY);
        self.results_sender = Some(tx);
        spawn(async move {
            loop {
                if let Err(err) = self.tick().await {
                    error!("parse aggregator tick error: {err:#}")
                }
            }
        });
        rx
    }

    async fn tick(&mut self) -> anyhow::Result<()> {
        let sender = self
            .results_sender
            .as_ref()
            .expect("invariant: no results sender");
        let input = self.input_receiver.recv().await.ok_or_else(|| {
            anyhow::anyhow!("no block could be received: channel unexpectedly closed")
        })?;
        let slot = input.slot;
        let tx_to_parse = TransactionToParse {
            transaction: input.transaction,
            inner_instructions: input.meta.inner_instructions,
        };
        let parser = Parser {
            config: self.config.clone(),
        };

        let result = spawn_blocking(move || -> anyhow::Result<anyhow::Result<Vec<_>>> {
            parser.parse_tx(tx_to_parse).map(|iter| iter.collect())
        })
        .await?
        .context("parsing block resulted in error")??;

        if result.is_empty() {
            debug!("ignored transaction at slot {slot}: no needed events");
            return Ok(());
        }

        sender
            .send(ParseResult {
                slot_number: slot,
                events: result,
            })
            .await?;

        Ok(())
    }
}

#[derive(Clone)]
struct Parser {
    config: Arc<ChainSyncConfig>,
}

impl Parser {
    fn parse_tx(
        self,
        mut tx: TransactionToParse,
    ) -> anyhow::Result<impl ParallelIterator<Item = anyhow::Result<TrackedEvent>>> {
        let accounts = take_static_keys(&mut tx.transaction)?;
        let instructions = tx.inner_instructions;
        Ok(instructions
            .into_par_iter()
            .flat_map(|instruction| instruction.instructions.into_par_iter())
            .filter_map(move |instruction| {
                Self::parse_instruction(&accounts, instruction).transpose()
            })
            .filter(move |event| {
                if let Err(err) = event.as_ref() {
                    tracing::trace!("error occurred with event: {err:#}");
                }
                let Ok(event) = event else { return true };
                let TrackedEvent::Pumpfun(event) = event else {
                    return true;
                };
                self.filter_pumpfun_event(event)
            }))
    }

    #[instrument(level = "debug")]
    fn parse_instruction(
        static_account_keys: &[Pubkey],
        instruction: InnerInstruction,
    ) -> anyhow::Result<Option<TrackedEvent>> {
        let program_id = static_account_keys[instruction.program_id_index as usize];
        if !TRACKED_PROGRAMS.contains(&program_id) {
            return Ok(None);
        }

        let Ok((discriminator, mut data)) = unpack_event_data(&instruction.data) else {
            return Ok(None);
        };

        Ok(match program_id {
            moonzip::ID_CONST => {
                let mzip_event: MoonzipEvent = match discriminator {
                    PROJECT_CHANGED_EVENT => ProjectChangedEvent::deserialize(&mut data)?.into(),
                    STATIC_POOL_SELL_EVENT => StaticPoolSellEvent::deserialize(&mut data)?.into(),
                    STATIC_POOL_BUY_EVENT => StaticPoolBuyEvent::deserialize(&mut data)?.into(),
                    CURVE_POOL_BUY_EVENT => CurvedPoolBuyEvent::deserialize(&mut data)?.into(),
                    CURVE_POOL_SELL_EVENT => CurvedPoolSellEvent::deserialize(&mut data)?.into(),
                    _ => bail!("unsupported moonzip event discriminator: {discriminator:?}"),
                };
                Some(TrackedEvent::from(mzip_event))
            }
            pumpfun_cpi::ID_CONST => Some(TrackedEvent::from(PumpfunEvent::from(
                match discriminator {
                    TRADE_EVENT_DISCRIMINATOR => pumpfun_cpi::TradeEvent::deserialize(&mut data)?,
                    _ => bail!("unsupported pumpfun event discriminator: {discriminator:?}"),
                },
            ))),
            _ => bail!("invariant: program must be filtered in advance"),
        })
    }

    fn filter_pumpfun_event(&self, event: &PumpfunEvent) -> bool {
        match event {
            PumpfunEvent::Trade(trade_event) => self
                .config
                .allowed_mint_suffix
                .as_ref()
                .map(|filter| trade_event.mint.to_string().ends_with(filter))
                .unwrap_or(true),
        }
    }
}

struct TransactionToParse {
    inner_instructions: Vec<InnerInstructions>,
    transaction: Transaction,
}

fn take_static_keys(tx: &mut Transaction) -> anyhow::Result<Vec<Pubkey>> {
    tx.message
        .take()
        .ok_or_else(|| anyhow::anyhow!("no message in transaction"))?
        .account_keys
        .into_iter()
        .map(|key| {
            Pubkey::try_from(key)
                .map_err(|key| anyhow::anyhow!("failed to deserialize pubkey {key:?}"))
        })
        .collect()
}

fn unpack_event_data(data: &[u8]) -> anyhow::Result<(&[u8], &[u8])> {
    if data.len() <= ANCHOR_DISCRIMINATOR_BYTE_SIZE * 2 {
        bail!("event instruction should contain at least two discriminators");
    }
    let (discriminator, event_data) = data.split_at(ANCHOR_DISCRIMINATOR_BYTE_SIZE);
    if discriminator != EVENT_IX_TAG_LE {
        bail!(
            "event discriminator mismatch, got: {discriminator:?}, expected: {EVENT_IX_TAG_LE:?}"
        );
    }
    let (event_discriminator, event_data) = event_data.split_at(ANCHOR_DISCRIMINATOR_BYTE_SIZE);
    Ok((event_discriminator, event_data))
}

#[derive(Debug, derive_more::From)]
pub enum TrackedEvent {
    Pumpfun(PumpfunEvent),
    Moonzip(MoonzipEvent),
}

#[derive(Debug, derive_more::From)]
pub enum MoonzipEvent {
    ProjectChanged(ProjectChangedEvent),

    StaticPoolBuy(StaticPoolBuyEvent),
    StaticPoolSell(StaticPoolSellEvent),

    CurvedPoolBuy(CurvedPoolBuyEvent),
    CurvedPoolSell(CurvedPoolSellEvent),
}

#[derive(Debug, derive_more::From)]
pub enum PumpfunEvent {
    Trade(pumpfun_cpi::TradeEvent),
}
