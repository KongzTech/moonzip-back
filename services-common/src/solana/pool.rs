use crate::utils::{
    keypair::SaneKeypair,
    limiter::{LimiterGuard, RateLimitConfig},
};
use once_cell::sync::Lazy;
use serde::Deserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use std::sync::{atomic, Arc};

#[derive(Deserialize, Debug, Clone)]
pub struct SolanaPoolConfig {
    pub clients: Vec<SolanaClientConfig>,
}

#[derive(Clone)]
pub struct SolanaPool {
    clients: Arc<Balancer<SolanaClient>>,
}

impl SolanaPool {
    pub fn from_cfg(cfg: SolanaPoolConfig) -> anyhow::Result<Self> {
        let clients = cfg
            .clients
            .iter()
            .map(|client_cfg| SolanaClient::new(client_cfg.clone()))
            .collect::<Vec<_>>();
        Ok(Self {
            clients: Arc::new(Balancer::new(clients)),
        })
    }

    pub fn client(&self) -> &SolanaClient {
        self.clients.next()
    }

    pub fn builder(&self) -> anchor_client::Client<SaneKeypair> {
        static ANY_KEYPAIR: Lazy<SaneKeypair> = Lazy::new(|| SaneKeypair::from(Keypair::new()));
        anchor_client::Client::new(anchor_client::Cluster::Debug, (*ANY_KEYPAIR).clone())
    }
}

pub struct SolanaClient {
    rpc_client: LimiterGuard<RpcClient>,
}

impl SolanaClient {
    pub fn new(config: SolanaClientConfig) -> Self {
        let rpc_client = RpcClient::new(config.node.rpc_url());
        let rpc_client = LimiterGuard::new(rpc_client, config.limit.limiter());
        Self { rpc_client }
    }

    pub fn rpc(&self) -> &LimiterGuard<RpcClient> {
        &self.rpc_client
    }
}

#[derive(Deserialize, Debug, Clone)]
pub struct SolanaClientConfig {
    #[serde(default)]
    limit: RateLimitConfig,
    node: NodeType,
}

#[derive(Deserialize, Debug, Clone)]
#[serde(tag = "type")]
#[serde(rename_all = "lowercase")]
pub enum NodeType {
    Helius { api_key: String },
    Local,
}

impl NodeType {
    pub fn rpc_url(&self) -> String {
        match self {
            NodeType::Helius { api_key } => {
                format!("https://mainnet.helius-rpc.com?api-key={}", api_key)
            }
            NodeType::Local => "http://localhost:8899".to_string(),
        }
    }

    pub fn ws_url(&self) -> String {
        match self {
            NodeType::Helius { api_key } => {
                format!("wss://mainnet.helius-rpc.com?api-key={}", api_key)
            }
            NodeType::Local => "ws://localhost:8900".to_string(),
        }
    }
}

struct Balancer<T> {
    data: Vec<T>,
    current_pos: atomic::AtomicUsize,
}

impl<T> Balancer<T> {
    fn new(data: Vec<T>) -> Self {
        Self {
            data,
            current_pos: atomic::AtomicUsize::default(),
        }
    }

    fn next(&self) -> &T {
        let length = self.data.len();
        let pos = self
            .current_pos
            .fetch_update(atomic::Ordering::SeqCst, atomic::Ordering::SeqCst, |x| {
                Some((x + 1) % length)
            })
            .expect("invariant: always ok");
        &self.data[pos]
    }
}
