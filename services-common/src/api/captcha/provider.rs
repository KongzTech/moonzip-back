use serde::Deserialize;

#[derive(Deserialize, Debug, Clone)]
pub struct CaptchaConfig {
    pub secret_key: String,
    pub enable_verify: bool,
}

#[derive(Clone)]
pub struct CaptchaProvider {
    pub secret_key: String,
    pub enable_verify: bool,
}

impl super::CaptchaProvider for CaptchaProvider {
    fn secret_key(&self) -> &String {
        &self.secret_key
    }

    fn enable_verify(&self) -> bool {
        self.enable_verify
    }
}

impl CaptchaProvider {
    pub fn from_cfg(cfg: CaptchaConfig) -> Self {
        Self {
            secret_key: cfg.secret_key,
            enable_verify: cfg.enable_verify,
        }
    }
}
