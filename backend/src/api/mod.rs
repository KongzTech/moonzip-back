use crate::app::{App, CreateProjectRequest, CreateProjectResponse};
use axum::{extract::State, routing::post, Json, Router};
use services_common::api::response::{ApiError, AppJson};

pub type BackendState = services_common::api::server::AppState<App>;

pub fn router() -> Router<BackendState> {
    Router::new().route("/project/create", post(create_project))
}

pub async fn create_project(
    State(state): State<BackendState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<AppJson<CreateProjectResponse>, ApiError> {
    Ok(AppJson(state.app().create_project(request).await?))
}
