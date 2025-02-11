use crate::app::{
    exposed::{
        BuyRequest, BuyResponse, CreateProjectForm, CreateProjectResponse, CreateProjectStreamData,
        SellRequest, SellResponse,
    },
    exposed::{GetProjectRequest, GetProjectResponse},
    App,
};
use axum::{
    extract::{DefaultBodyLimit, Multipart, Query, State},
    routing::{get, post},
    Json, Router,
};
use futures_util::TryStreamExt;
use services_common::api::captcha::Captcha;
use services_common::api::response::{ApiError, AppJson, ErrorResponse};
use tokio_util::io::StreamReader;
use utoipa::OpenApi;
use utoipauto::utoipauto;

#[utoipauto(paths = "./backend/src")]
#[derive(OpenApi)]
#[openapi()]
pub struct ApiDoc;

pub type BackendState = services_common::api::server::AppState<App>;

pub fn router() -> Router<BackendState> {
    Router::new()
        .nest(
            "/project",
            Router::new()
                .route("/create", post(create_project))
                .route("/buy", post(buy))
                .route("/sell", post(sell))
                .route("/get", get(get_project)),
        )
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
