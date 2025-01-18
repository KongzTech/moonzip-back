use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::Deserialize;
use std::time::Duration;

#[derive(Deserialize, Debug, Clone)]
pub struct AuthConfig {
    pub decoding_key: String,
    pub encoding_key: String,

    #[serde(with = "humantime_serde", default = "default_token_ttl")]
    pub token_ttl: Duration,
}

fn default_token_ttl() -> Duration {
    // every 6 hours
    Duration::from_secs(60 * 60 * 6)
}

#[derive(Clone)]
pub struct AuthProvider {
    pub decoding_key: DecodingKey,
    pub encoding_key: EncodingKey,
    pub token_ttl: Duration,
}

impl super::ConfigProvider for AuthProvider {
    fn decode_key(&self) -> &DecodingKey {
        &self.decoding_key
    }

    fn encode_key(&self) -> &EncodingKey {
        &self.encoding_key
    }

    fn token_ttl(&self) -> Duration {
        self.token_ttl
    }
}

impl AuthProvider {
    pub fn from_cfg(cfg: AuthConfig) -> Self {
        Self {
            decoding_key: DecodingKey::from_secret(cfg.decoding_key.as_bytes()),
            encoding_key: EncodingKey::from_secret(cfg.encoding_key.as_bytes()),
            token_ttl: cfg.token_ttl,
        }
    }
}
