use super::jito::{JitoClient, JitoClientConfig};
use crate::utils::{
    keypair::SaneKeypair,
    limiter::{LimiterGuard, RateLimitConfig},
};
use derive_more::derive::Deref;
use once_cell::sync::Lazy;
use serde::Deserialize;
use solana_client::nonblocking::rpc_client::RpcClient;
use solana_sdk::signature::Keypair;
use std::sync::{atomic, Arc};

#[derive(Deserialize, Debug, Clone)]
pub struct SolanaPoolConfig {
    pub rpc_clients: Vec<SolanaClientConfig>,
    pub jito_clients: Vec<JitoClientConfig>,
}

#[derive(Clone)]
pub struct SolanaPool {
    rpc_clients: Arc<Balancer<SolanaRpcClient>>,
    jito_clients: Arc<Balancer<JitoClient>>,
}

impl SolanaPool {
    pub fn from_cfg(cfg: SolanaPoolConfig) -> anyhow::Result<Self> {
        let rpc_clients = cfg
            .rpc_clients
            .iter()
            .map(|client_cfg| SolanaRpcClient::new(client_cfg.clone()))
            .collect::<Vec<_>>();
        let jito_clients = cfg
            .jito_clients
            .iter()
            .map(|client_cfg| JitoClient::new(client_cfg.clone()))
            .collect::<Vec<_>>();
        Ok(Self {
            rpc_clients: Arc::new(Balancer::new(rpc_clients)),
            jito_clients: Arc::new(Balancer::new(jito_clients)),
        })
    }

    pub fn rpc_client(&self) -> &SolanaRpcClient {
        self.rpc_clients.next()
    }

    pub fn jito_client(&self) -> &JitoClient {
        self.jito_clients.next()
    }

    pub fn builder(&self) -> anchor_client::Client<SaneKeypair> {
        static ANY_KEYPAIR: Lazy<SaneKeypair> = Lazy::new(|| SaneKeypair::from(Keypair::new()));
        anchor_client::Client::new(anchor_client::Cluster::Debug, (*ANY_KEYPAIR).clone())
    }
}

#[derive(Deref)]
pub struct SolanaRpcClient {
    rpc_client: LimiterGuard<RpcClient>,
}

impl SolanaRpcClient {
    pub fn new(config: SolanaClientConfig) -> Self {
        let rpc_client = RpcClient::new(config.node.rpc_url());
        let rpc_client = LimiterGuard::new(rpc_client, config.limit.limiter());
        Self { rpc_client }
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
    Any { rpc_url: String },
}

impl NodeType {
    pub fn rpc_url(&self) -> String {
        match self {
            NodeType::Helius { api_key } => {
                format!("https://mainnet.helius-rpc.com?api-key={}", api_key)
            }
            NodeType::Any { rpc_url } => rpc_url.clone(),
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
