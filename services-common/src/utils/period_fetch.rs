use serde::Deserialize;
use std::{marker::PhantomData, time::Duration};
use tokio::sync::watch;
use tracing::error;

#[derive(Clone)]
pub struct DataReceiver<T>(watch::Receiver<Option<T>>);

impl<T: Clone> DataReceiver<T> {
    pub async fn get(&mut self) -> anyhow::Result<T> {
        Ok(self
            .0
            .wait_for(|meta| meta.is_some())
            .await?
            .as_ref()
            .map(|meta| meta.clone())
            .expect("invariant: waited for some"))
    }
}

#[async_trait::async_trait]
pub trait FetchExecutor<T> {
    async fn init(&mut self) -> anyhow::Result<()>;
    async fn fetch(&mut self) -> anyhow::Result<T>;
    fn name(&self) -> &'static str;
}

pub struct PeriodicFetcher<T, E> {
    executor: E,
    config: PeriodicFetcherConfig,
    _marker: PhantomData<T>,
}

#[derive(Clone, Deserialize, serde_derive_default::Default)]
pub struct PeriodicFetcherConfig {
    #[serde(with = "humantime_serde", default = "default_tick_interval")]
    pub tick_interval: Duration,
    #[serde(with = "humantime_serde", default = "default_error_backoff")]
    pub error_backoff: Duration,
}

impl PeriodicFetcherConfig {
    pub fn zero() -> Self {
        Self {
            tick_interval: Duration::ZERO,
            error_backoff: Duration::ZERO,
        }
    }

    pub fn every_hour() -> Self {
        Self {
            tick_interval: Duration::from_secs(60 * 60),
            error_backoff: Duration::from_secs(5),
        }
    }
}

fn default_tick_interval() -> Duration {
    Duration::from_secs(60)
}

fn default_error_backoff() -> Duration {
    Duration::from_secs(3)
}

impl<T: Send + Sync + 'static + PartialOrd, E: FetchExecutor<T> + Send + Sync + 'static>
    PeriodicFetcher<T, E>
{
    pub fn new(executor: E, config: PeriodicFetcherConfig) -> Self {
        Self {
            executor,
            config,
            _marker: PhantomData,
        }
    }

    pub fn serve(mut self) -> DataReceiver<T> {
        let (sender, receiver) = watch::channel(None);
        tokio::spawn(async move {
            if let Err(e) = self.executor.init().await {
                error!("failed to init fetcher {}: {}", self.executor.name(), e);
                return;
            }
            loop {
                match self.tick(&sender).await {
                    Ok(_) => {
                        tokio::time::sleep(self.config.tick_interval).await;
                    }
                    Err(e) => {
                        error!("failed to fetch data from {}: {}", self.executor.name(), e);
                        tokio::time::sleep(self.config.error_backoff).await;
                    }
                }
            }
        });
        DataReceiver(receiver)
    }

    async fn tick(&mut self, sender: &watch::Sender<Option<T>>) -> anyhow::Result<()> {
        let mut data = self.executor.fetch().await?;
        sender.send_if_modified(move |old| {
            let Some(old) = old else {
                return false;
            };
            let modified = old < &mut data;
            if modified {
                *old = data;
            }
            modified
        });
        Ok(())
    }
}
