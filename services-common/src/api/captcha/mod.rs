use crate::api::response::ApiError;
use anyhow::anyhow;
use axum::extract::FromRequestParts;
use axum::RequestPartsExt;
use constants::CAPTCHA_HEADER_NAME;
use constants::CLOUDFLARE_VERIFY_URL;
use http::request::Parts;
use http::HeaderMap;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use serde_json::json;
use tracing::trace;

pub mod constants;
pub mod provider;

pub trait CaptchaProvider {
    fn secret_key(&self) -> &String;
    fn enable_verify(&self) -> bool;
}

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct Captcha {
    pub is_success: bool,
}

async fn decode_header(parts: &mut Parts) -> anyhow::Result<String> {
    let path = parts.extract::<HeaderMap>().await?;
    let value = path
        .get(CAPTCHA_HEADER_NAME)
        .ok_or_else(|| anyhow!("no captcha"))?;
    let value = value.to_str()?;
    Ok(value.to_string())
}

impl<S: Send + Sync + CaptchaProvider> FromRequestParts<S> for Captcha {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        if !state.enable_verify() {
            return Ok(Captcha { is_success: true });
        }

        let captcha_response = decode_header(parts).await.map_err(|err| {
            trace!("user supplied invalid captcha header: {err:#}");
            ApiError::InvalidCaptcha(anyhow::anyhow!("Missing CAPTCHA"))
        })?;
        trace!("user supplied captcha_response: {captcha_response:?}");

        let url = CLOUDFLARE_VERIFY_URL;
        let params = [
            ("secret", &state.secret_key()),
            ("response", &&captcha_response),
        ];

        let http_client = Client::new();
        let res = http_client.post(url).form(&params).send().await;

        match res {
            Ok(response) => {
                let json: serde_json::Value = response
                    .json()
                    .await
                    .unwrap_or_else(|_| json!({"success": false}));
                let is_success = json["success"].as_bool().unwrap_or(false);

                if !is_success {
                    return Err(ApiError::InvalidCaptcha(anyhow::anyhow!(
                        "CAPTCHA_HEADER verification failed"
                    )));
                }

                Ok(Captcha { is_success })
            }
            Err(_) => Err(ApiError::InvalidRequest(anyhow::anyhow!(
                "CAPTCHA verification service unavailable"
            ))),
        }
    }
}
