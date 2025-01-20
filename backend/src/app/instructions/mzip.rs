use anchor_client::anchor_lang::AnchorDeserialize;
use moonzip::moonzip::GlobalCurvedPoolAccount;
use once_cell::sync::Lazy;
use services_common::solana::pool::SolanaPool;
use solana_sdk::pubkey::Pubkey;
use std::time::Duration;
use tokio::sync::watch;
use tracing::error;

#[derive(Clone)]
pub struct MetaReceiver(watch::Receiver<Option<Meta>>);

impl MetaReceiver {
    pub async fn get(&mut self) -> anyhow::Result<Meta> {
        Ok(self
            .0
            .wait_for(|meta| meta.is_some())
            .await?
            .as_ref()
            .map(|meta| meta.clone())
            .expect("invariant: waited for some"))
    }
}

pub struct MetaFetcher {
    pool: SolanaPool,
}

static META_TICK_INTERVAL: Lazy<Duration> = Lazy::new(|| Duration::from_secs(60) * 60 * 12);
static META_ERROR_BACKOFF: Lazy<Duration> = Lazy::new(|| Duration::from_secs(3));

impl MetaFetcher {
    pub fn new(pool: SolanaPool) -> Self {
        Self { pool }
    }

    pub fn serve(self) -> MetaReceiver {
        let (sender, receiver) = watch::channel(None);
        tokio::spawn(async move {
            loop {
                match self.tick(&sender).await {
                    Ok(_) => {
                        tokio::time::sleep(*META_TICK_INTERVAL).await;
                    }
                    Err(e) => {
                        error!("failed to fetch pumpfun meta: {}", e);
                        tokio::time::sleep(*META_ERROR_BACKOFF).await;
                    }
                }
            }
        });
        MetaReceiver(receiver)
    }

    async fn tick(&self, sender: &watch::Sender<Option<Meta>>) -> anyhow::Result<()> {
        let meta = self.fetch().await?;
        sender.send(Some(meta))?;
        Ok(())
    }

    async fn fetch(&self) -> anyhow::Result<Meta> {
        let client = self.pool.client().rpc().use_single().await;
        let global = client.get_account_data(&GLOBAL_ACCOUNT).await?;
        Ok(Meta {
            global_account: GlobalCurvedPoolAccount::try_from_slice(&global)?,
        })
    }
}

#[derive(Clone)]
pub struct Meta {
    pub global_account: GlobalCurvedPoolAccount,
}

pub static GLOBAL_ACCOUNT: Lazy<Pubkey> = Lazy::new(global_account_address);

fn global_account_address() -> Pubkey {
    Pubkey::find_program_address(&[b"curved-pool-global-account"], &moonzip::ID).0
}
