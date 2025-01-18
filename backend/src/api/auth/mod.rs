use std::str::FromStr;
use std::time::Duration;

use anyhow::anyhow;
use anyhow::bail;
use anyhow::Context;
use axum::extract::State;
use axum::{
    extract::FromRequestParts,
    http::{request::Parts, HeaderMap},
    Json, RequestPartsExt,
};
use chrono::DateTime;
use http::header::AUTHORIZATION;
use jsonwebtoken::Algorithm;
use jsonwebtoken::DecodingKey;
use jsonwebtoken::EncodingKey;
use jsonwebtoken::Header;
use jsonwebtoken::Validation;
use reqwest::StatusCode;
use serde::{Deserialize, Serialize};
use serde_with::{serde_as, DisplayFromStr};
use solana_sdk::pubkey::Pubkey;
use solana_sdk::signature::Signature;
use tracing::trace;
use utoipa::ToSchema;

use crate::utils::serde_timestamp;
use crate::TZ;

use super::response::ApiError;
use super::response::AppJson;
use super::response::ErrorResponse;
pub mod provider;

#[utoipa::path(
        post,
        tag = "auth",
        path = "/api/auth",
        responses(
            (status = 200, description = "Succesfully authenticated and received token", body = AuthPropose),
            ErrorResponse
        )
    )]
pub async fn auth<S: ConfigProvider>(
    State(state): State<S>,
    Json(request): Json<AuthRequest>,
) -> Result<AppJson<AuthPropose>, ApiError> {
    let expires_at = TZ::now() + state.token_ttl();
    let token = jsonwebtoken::encode(
        &Header::default(),
        &Claims {
            sub: request.user,
            exp: expires_at,
        },
        state.encode_key(),
    )
    .context("encode token")?;
    Ok(AppJson(AuthPropose {
        token: token.to_string(),
        expires_at,
    }))
}

pub trait ConfigProvider {
    fn decode_key(&self) -> &DecodingKey;
    fn encode_key(&self) -> &EncodingKey;
    fn token_ttl(&self) -> Duration;
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Clone, ToSchema)]
pub struct AuthRequest {
    #[serde_as(as = "DisplayFromStr")]
    #[schema(value_type = String)]
    pub user: Pubkey,
}

#[derive(Deserialize, Serialize, Debug, Clone, ToSchema)]
pub struct AuthPropose {
    pub token: String,
    #[serde(with = "serde_timestamp")]
    #[schema(value_type = u64)]
    pub expires_at: DateTime<TZ>,
}

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error("token is expired, please regenerate")]
    TokenExpired,

    #[error("signature mismatch for message")]
    SignatureMismatch,

    #[error("no authorization header or it is malformed")]
    InvalidHeaders,

    #[error("passed token is malformed")]
    MalformedToken,
}

impl From<Error> for Rejection {
    fn from(value: Error) -> Self {
        let code = match value {
            Error::TokenExpired => 4030,
            Error::InvalidHeaders => 4031,
            Error::MalformedToken => 4032,
            Error::SignatureMismatch => 4033,
        };

        (
            StatusCode::UNAUTHORIZED,
            Json(ErrorResponse {
                code,
                message: value.to_string(),
            }),
        )
    }
}

type Rejection = (StatusCode, axum::Json<ErrorResponse>);

#[derive(Deserialize, Serialize, Debug, Clone)]
pub struct User {
    pub key: Pubkey,
}

#[serde_as]
#[derive(Deserialize, Serialize, Debug, Clone)]
struct Claims {
    #[serde_as(as = "DisplayFromStr")]
    sub: Pubkey,
    #[serde(with = "serde_timestamp")]
    exp: DateTime<TZ>,
}

#[derive(Debug, Clone)]
struct AuthToken {
    token: String,
    signature: Signature,
}

async fn decode_header(parts: &mut Parts) -> anyhow::Result<AuthToken> {
    const DELIMITER: &str = ";";

    let path = parts.extract::<HeaderMap>().await?;
    let value = path
        .get(AUTHORIZATION)
        .ok_or_else(|| anyhow!("no authorization header"))?;
    let value = value.to_str()?;
    let Some((token, signature)) = value.split_once(DELIMITER) else {
        bail!("no {DELIMITER} found, separate correctly")
    };
    Ok(AuthToken {
        token: token.to_owned(),
        signature: Signature::from_str(signature).context("decode signature")?,
    })
}

impl<S: Send + Sync + ConfigProvider> FromRequestParts<S> for User {
    type Rejection = Rejection;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let token = decode_header(parts).await.map_err(|err| {
            trace!("user supplied invalid auth header: {err:#}");
            Error::InvalidHeaders
        })?;
        trace!("user supplied token: {token:?}");
        let decoded = jsonwebtoken::decode::<Claims>(
            &token.token,
            state.decode_key(),
            &Validation::new(Algorithm::HS256),
        )
        .map_err(|err| {
            trace!("failed to decode jwt: {err:#}");
            Error::MalformedToken
        })?;
        let claims = decoded.claims;

        if claims.exp < TZ::now() {
            return Err(Error::TokenExpired.into());
        }
        let verified = token
            .signature
            .verify(claims.sub.as_ref(), token.token.as_bytes());
        if !verified {
            return Err(Error::SignatureMismatch.into());
        }
        Ok(User { key: claims.sub })
    }
}

#[cfg(test)]
mod tests {
    use std::{future::Future, time::Duration};

    use axum::{
        routing::{get, post},
        Router,
    };
    use http::{header::AUTHORIZATION, StatusCode};
    use provider::{AuthConfig, AuthProvider};
    use solana_sdk::{signature::Keypair, signer::Signer};
    use tokio::{
        runtime::Runtime,
        sync::{oneshot, OnceCell},
        time::{sleep, Instant},
    };

    use crate::{log::setup_log, utils::decode_response_type_or_raw};

    use super::*;
    use tracing::{error, info};

    async fn wait_active(port: u16) -> anyhow::Result<()> {
        let check = || async {
            let status = reqwest::get(format!("http://localhost:{port}/health"))
                .await?
                .status();
            if status != StatusCode::OK {
                anyhow::bail!("not ok: {status:?}");
            }
            Ok(())
        };
        let start = Instant::now();
        while check().await.is_err() {
            sleep(Duration::from_secs(1)).await;
            if start.elapsed() > Duration::from_secs(10) {
                anyhow::bail!("deadline waiting for server to become active");
            }
        }
        Ok(())
    }

    #[derive(Deserialize, Serialize, Debug, Clone, PartialEq, Eq)]
    pub struct SampleRequest {
        raw: String,
    }

    #[serde_as]
    #[derive(Deserialize, Serialize, Debug, Clone)]
    pub struct SampleResponse {
        #[serde_as(as = "DisplayFromStr")]
        user: Pubkey,
        request: SampleRequest,
    }

    type AppState = AuthProvider;

    pub async fn handler(
        User { key }: User,
        Json(request): Json<SampleRequest>,
    ) -> Result<AppJson<SampleResponse>, ApiError> {
        Ok(AppJson(SampleResponse { user: key, request }))
    }

    #[derive(Deserialize, Serialize, Debug, Clone)]
    struct HealthResponse {
        status: bool,
    }

    async fn health() -> Result<AppJson<HealthResponse>, ApiError> {
        Ok(AppJson(HealthResponse { status: true }))
    }

    async fn serve(token_ttl: Duration, port_tx: oneshot::Sender<u16>) -> anyhow::Result<()> {
        let api = Router::new()
            .route("/auth", post(auth::<AppState>))
            .route("/auth_test", post(handler))
            .route("/health", get(health))
            .with_state(AppState::from_cfg(AuthConfig {
                decoding_key: "OCwwEOFJtv2m7drF6v7AZwFPiv+B24GD7kBlgsYGB0U=".into(),
                encoding_key: "OCwwEOFJtv2m7drF6v7AZwFPiv+B24GD7kBlgsYGB0U=".into(),
                token_ttl,
            }));
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr()?.port();
        port_tx.send(port).unwrap();
        axum::serve(listener, api).await?;
        Result::<(), anyhow::Error>::Ok(())
    }

    async fn prepare() -> String {
        setup_log();
        let token_ttl = Duration::from_secs(3);

        let (port_tx, port_rx) = tokio::sync::oneshot::channel();
        tokio::spawn(async move {
            if let Err(err) = serve(token_ttl, port_tx).await {
                error!("auth server failed to start: {err:#}");
            }
        });

        let port = port_rx.await.unwrap();
        let endpoint = format!("http://127.0.0.1:{port}");
        info!("auth server is running on {endpoint}");
        wait_active(port).await.unwrap();
        endpoint
    }

    static SERVER: OnceCell<String> = OnceCell::const_new();
    async fn init() -> String {
        SERVER.get_or_init(prepare).await.to_owned()
    }

    static RUNTIME: once_cell::sync::Lazy<Runtime> =
        once_cell::sync::Lazy::new(|| Runtime::new().expect("Failed to create Tokio runtime"));

    fn run_test(test: impl Future<Output = anyhow::Result<()>>) -> anyhow::Result<()> {
        RUNTIME.block_on(test)?;
        Ok(())
    }

    #[test]
    fn test_without_header() -> anyhow::Result<()> {
        run_test(async move {
            let base_url = init().await;
            let client = reqwest::Client::new();
            let request = SampleRequest {
                raw: "message".into(),
            };
            let result = client
                .post(format!("{base_url}/auth_test"))
                .json(&request)
                .send()
                .await?;

            assert_eq!(result.status(), 401, "result incorrect status: {result:?}");
            assert_eq!(
                result.json::<ErrorResponse>().await?,
                ErrorResponse {
                    code: 4031,
                    message: "no authorization header or it is malformed".into()
                }
            );
            Ok(())
        })
    }

    #[test]
    fn test_happy_path() -> anyhow::Result<()> {
        run_test(async move {
            let base_url = init().await;
            let client = reqwest::Client::new();
            let request = SampleRequest {
                raw: "message".into(),
            };

            let keypair = Keypair::new();
            let auth_result: AuthPropose = client
                .post(format!("{base_url}/auth"))
                .json(&AuthRequest {
                    user: keypair.pubkey(),
                })
                .send()
                .await?
                .json()
                .await?;
            let signature = keypair
                .sign_message(auth_result.token.as_bytes())
                .to_string();
            let pair = format!("{};{signature}", auth_result.token);

            let result: SampleResponse = decode_response_type_or_raw(
                client
                    .post(format!("{base_url}/auth_test"))
                    .header(AUTHORIZATION, pair)
                    .json(&request)
                    .send()
                    .await?,
            )
            .await?;

            assert_eq!(result.user, keypair.pubkey());
            assert_eq!(result.request, request);

            Ok(())
        })
    }

    #[test]
    fn test_signature_mismatch() -> anyhow::Result<()> {
        run_test(async move {
            let base_url = init().await;
            let client = reqwest::Client::new();
            let request = SampleRequest {
                raw: "message".into(),
            };

            let keypair = Keypair::new();
            let auth_result: AuthPropose = client
                .post(format!("{base_url}/auth"))
                .json(&AuthRequest {
                    user: keypair.pubkey(),
                })
                .send()
                .await?
                .json()
                .await?;
            let signature = Keypair::new()
                .sign_message(auth_result.token.as_bytes())
                .to_string();
            let pair = format!("{};{signature}", auth_result.token);

            let result = client
                .post(format!("{base_url}/auth_test"))
                .header(AUTHORIZATION, pair)
                .json(&request)
                .send()
                .await?;

            assert_eq!(result.status(), 401, "result incorrect status: {result:?}");
            assert_eq!(
                result.json::<ErrorResponse>().await?,
                ErrorResponse {
                    code: 4033,
                    message: "signature mismatch for message".into()
                }
            );

            Ok(())
        })
    }

    #[test]
    fn test_expiration() -> anyhow::Result<()> {
        run_test(async move {
            let base_url = init().await;
            let client = reqwest::Client::new();
            let request = SampleRequest {
                raw: "message".into(),
            };

            let keypair = Keypair::new();
            let auth_result: AuthPropose = client
                .post(format!("{base_url}/auth"))
                .json(&AuthRequest {
                    user: keypair.pubkey(),
                })
                .send()
                .await?
                .json()
                .await?;
            let signature = keypair
                .sign_message(auth_result.token.as_bytes())
                .to_string();
            let pair = format!("{};{signature}", auth_result.token);

            sleep(Duration::from_secs(3)).await;
            let result = client
                .post(format!("{base_url}/auth_test"))
                .header(AUTHORIZATION, pair)
                .json(&request)
                .send()
                .await?;

            assert_eq!(result.status(), 401, "result incorrect status: {result:?}");
            assert_eq!(
                result.json::<ErrorResponse>().await?,
                ErrorResponse {
                    code: 4030,
                    message: "token is expired, please regenerate".into()
                }
            );

            Ok(())
        })
    }
}
