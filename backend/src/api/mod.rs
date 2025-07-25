use crate::app::exposed::{
    ChangeUserInfoRequest, GetOwnedNFTsRequest, GetUserInformationRequest, UserInfo,
};
use crate::app::{
    exposed::{
        BuyRequest, BuyResponse, CreateProjectForm, CreateProjectResponse, CreateProjectStreamData,
        DevLockClaimRequest, DevLockClaimResponse, GetProjectRequest, GetProjectResponse,
        SellRequest, SellResponse,
    },
    App,
};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    routing::{get, post},
    Json, Router,
};
use futures_util::TryStreamExt;
use http::Method;
use services_common::api::captcha::Captcha;
use services_common::api::response::{ApiError, AppJson, ErrorResponse};
use services_common::solana::helius::GetOwnedNFTsResponse;
use tokio_util::io::StreamReader;
use tower_http::cors::{Any, CorsLayer};
use utoipa::OpenApi;
use utoipauto::utoipauto;
use validator::Validate;

#[utoipauto(paths = "./backend/src")]
#[derive(OpenApi)]
#[openapi()]
pub struct ApiDoc;

pub type BackendState = services_common::api::server::AppState<App>;

pub fn router() -> Router<BackendState> {
    let cors = CorsLayer::new()
        .allow_methods([
            Method::GET,
            Method::POST,
            Method::PUT,
            Method::DELETE,
            Method::OPTIONS,
        ])
        .allow_origin(Any)
        .allow_headers([
            http::header::CONTENT_TYPE,
            http::header::AUTHORIZATION,
            http::header::ACCEPT,
        ]);

    Router::new()
        .nest(
            "/project",
            Router::new()
                .route("/create", post(create_project))
                .route("/buy", post(buy))
                .route("/sell", post(sell))
                .route("/claim_dev_lock", post(claim_dev_lock))
                .route("/get", get(get_project)),
        )
        .nest(
            "/user",
            Router::new()
                .route("/get", get(get_user_info))
                .route("/upsert", post(upsert_username))
                .route("/owned-nfts", get(get_nft_owned_by_user)),
        )
        .layer(cors)
        .layer(DefaultBodyLimit::max(1024 * 4))
}

#[utoipa::path(
    post,
    tag = "project",
    path = "/api/project/create",
    request_body(content = CreateProjectForm, content_type = "multipart/form-data"),
    responses(
        (status = 200, description = "Successfully created project", body = CreateProjectResponse),
        ErrorResponse
    )
)]
pub async fn create_project(
    State(state): State<BackendState>,
    _captcha: Captcha,
    mut multipart: Multipart,
) -> Result<AppJson<CreateProjectResponse>, ApiError> {
    let request = multipart
        .next_field()
        .await?
        .ok_or_else(|| {
            ApiError::InvalidRequest(anyhow::anyhow!("project create request data is missing"))
        })?
        .bytes()
        .await?;
    let request = serde_json::from_slice(&request).map_err(|e| {
        ApiError::InvalidRequest(anyhow::anyhow!(
            "failed to parse project create request: {}",
            e
        ))
    })?;

    let image_content = multipart
        .next_field()
        .await?
        .ok_or_else(|| ApiError::InvalidRequest(anyhow::anyhow!("image content is missing")))?;
    let image_content = StreamReader::new(
        image_content.map_err(|err| ApiError::InvalidRequest(anyhow::anyhow!(err))),
    );
    let streams = CreateProjectStreamData { image_content };

    Ok(AppJson(state.app().create_project(request, streams).await?))
}

#[utoipa::path(
    post,
    tag = "project",
    path = "/api/project/buy",
    responses(
        (status = 200, description = "Successfully bought tokens from project", body = BuyResponse),
        ErrorResponse
    )
)]
pub async fn buy(
    State(state): State<BackendState>,
    _captcha: Captcha,
    Json(request): Json<BuyRequest>,
) -> Result<AppJson<BuyResponse>, ApiError> {
    Ok(AppJson(state.app().buy(request).await?))
}

#[utoipa::path(
    post,
    tag = "project",
    path = "/api/project/sell",
    responses(
        (status = 200, description = "Successfully sold tokens to project", body = SellResponse),
        ErrorResponse
    )
)]
pub async fn sell(
    State(state): State<BackendState>,
    _captcha: Captcha,
    Json(request): Json<SellRequest>,
) -> Result<AppJson<SellResponse>, ApiError> {
    Ok(AppJson(state.app().sell(request).await?))
}

#[utoipa::path(
    post,
    tag = "project",
    path = "/api/project/claim_dev_lock",
    responses(
        (status = 200, description = "Provided transaction to claim dev tokens", body = DevLockClaimResponse),
        ErrorResponse
    )
)]
pub async fn claim_dev_lock(
    State(state): State<BackendState>,
    Json(request): Json<DevLockClaimRequest>,
) -> Result<AppJson<DevLockClaimResponse>, ApiError> {
    Ok(AppJson(state.app().dev_lock_claim(request).await?))
}

#[utoipa::path(
    get,
    tag = "project",
    path = "/api/project/get",
    params(GetProjectRequest),
    responses(
        (status = 200, description = "Successfully fetched project", body = GetProjectResponse),
        ErrorResponse
    )
)]
pub async fn get_project(
    State(state): State<BackendState>,
    Query(request): Query<GetProjectRequest>,
) -> Result<AppJson<GetProjectResponse>, ApiError> {
    Ok(AppJson(state.app().get_project(request).await?))
}

#[utoipa::path(
    get,
    tag = "user",
    path = "/api/user/get",
    responses(
        (status = 200, description = "Successfully get user information", body = UserInfo),
        ErrorResponse
    )
)]
pub async fn get_user_info(
    State(state): State<BackendState>,
    Query(request): Query<GetUserInformationRequest>,
) -> Result<AppJson<UserInfo>, ApiError> {
    Ok(AppJson(
        state.app().get_user_info_by_address(request).await?,
    ))
}

#[utoipa::path(
    post,
    tag = "user",
    path = "/api/user/upsert",
    responses(
        (status = 200, description = "Successfully change user information", body = UserInfo),
        ErrorResponse
    )
)]
pub async fn upsert_username(
    State(state): State<BackendState>,
    Json(request): Json<ChangeUserInfoRequest>,
) -> Result<AppJson<UserInfo>, ApiError> {
    request
        .validate()
        .map_err(|_| ApiError::InvalidRequest(anyhow::anyhow!("Invalid Parameter")))?;
    Ok(AppJson(state.app().upsert_user_info(request).await?))
}

#[utoipa::path(
    get,
    tag = "user",
    path = "/api/user/owned-nfts",
    responses(
        (status = 200, description = "Successfully retrieved NFTs", body = GetOwnedNFTsResponse),
        ErrorResponse
    )
)]
pub async fn get_nft_owned_by_user(
    State(state): State<BackendState>,
    Query(request): Query<GetOwnedNFTsRequest>,
) -> Result<AppJson<GetOwnedNFTsResponse>, ApiError> {
    request
        .validate()
        .map_err(|_| ApiError::InvalidRequest(anyhow::anyhow!("Invalid Parameter")))?;
    let response = state.app().get_owned_nfts_by_address(request).await?;
    Ok(AppJson(response))
}
