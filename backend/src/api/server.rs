use std::sync::Arc;

use super::auth;
use super::auth::provider::{AuthConfig, AuthProvider};
use super::response::{ApiError, AppJson};
use crate::app::{App, CreateProjectRequest, CreateProjectResponse};
use axum::extract::State;
use axum::Json;
use axum::{
    extract::{MatchedPath, Request},
    routing::{get, post},
    Router,
};
use jsonwebtoken::{DecodingKey, EncodingKey};
use serde::{Deserialize, Serialize};
use tokio::task::JoinSet;
use tower_http::trace::TraceLayer;
use utoipa::OpenApi;
use utoipa_rapidoc::RapiDoc;
use utoipa_redoc::{Redoc, Servable};
use utoipa_swagger_ui::SwaggerUi;
use utoipauto::utoipauto;

#[derive(Deserialize, Debug, Clone)]
pub struct Config {
    #[serde(default = "expose_docs_default")]
    pub expose_dev: bool,

    #[serde(default)]
    pub listen: ListenConfig,

    #[serde(default = "default_admin_listen")]
    pub admin_listen: ListenConfig,

    pub auth: AuthConfig,
}

#[derive(Deserialize, Debug, Clone, serde_derive_default::Default)]
pub struct ListenConfig {
    #[serde(default = "default_host")]
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u64,
}

impl ListenConfig {
    pub fn as_bind(&self) -> String {
        format!("{}:{}", self.host, self.port)
    }
}

pub fn default_host() -> String {
    "localhost".into()
}

pub fn default_port() -> u64 {
    8000
}

pub fn default_admin_listen() -> ListenConfig {
    ListenConfig {
        host: default_host(),
        port: 18000,
    }
}

pub fn expose_docs_default() -> bool {
    false
}

#[derive(Clone)]
pub struct AppState {
    app: Arc<App>,
    auth: Arc<AuthProvider>,
    config: Arc<Config>,
}

impl AppState {
    pub fn new(app: Arc<App>, config: Config) -> Self {
        Self {
            app,
            auth: Arc::new(AuthProvider::from_cfg(config.auth.clone())),
            config: Arc::new(config),
        }
    }
}

impl auth::ConfigProvider for AppState {
    fn decode_key(&self) -> &DecodingKey {
        &self.auth.decoding_key
    }

    fn encode_key(&self) -> &EncodingKey {
        &self.auth.encoding_key
    }

    fn token_ttl(&self) -> std::time::Duration {
        self.auth.token_ttl
    }
}

pub async fn serve(state: AppState) -> anyhow::Result<()> {
    #[utoipauto(paths = "./backend/src")]
    #[derive(OpenApi)]
    #[openapi()]
    struct ApiDoc;

    let api = Router::new()
        .route("/health", get(health))
        .route("/auth", post(auth::auth::<AppState>))
        .route("/project/create", post(create_project));

    let trace_layer = TraceLayer::new_for_http()
        .make_span_with(|req: &Request| {
            let method = req.method();
            let uri = req.uri();

            let matched_path = req
                .extensions()
                .get::<MatchedPath>()
                .map(|matched_path| matched_path.as_str());

            tracing::debug_span!("request", %method, %uri, matched_path)
        })
        .on_failure(());

    let app = Router::new();
    let app = if state.config.expose_dev {
        app.merge(
            SwaggerUi::new("/api/docs/swagger").url("/api/docs/openapi.json", ApiDoc::openapi()),
        )
        .merge(Redoc::with_url("/api/docs/redoc", ApiDoc::openapi()))
        .merge(RapiDoc::new("/api/docs/openapi.json").path("/api/docs/rapidoc"))
    } else {
        app
    };
    let app = app
        .nest("/api", api)
        .layer(trace_layer.clone())
        .with_state(state.clone());

    let admin_server = Router::new()
        .nest("/admin", Router::new())
        .with_state(state.clone());

    let mut set = JoinSet::new();
    let listen = state.config.listen.as_bind();
    let admin_listen = state.config.admin_listen.as_bind();
    set.spawn(async move {
        let listener = tokio::net::TcpListener::bind(listen).await.unwrap();
        tracing::debug!("listening api on {}", listener.local_addr().unwrap());
        axum::serve(listener, app).await.unwrap();
    });
    set.spawn(async move {
        let listener = tokio::net::TcpListener::bind(admin_listen).await.unwrap();
        tracing::debug!("listening admin on {}", listener.local_addr().unwrap());
        axum::serve(listener, admin_server).await.unwrap();
    });

    while let Some(result) = set.join_next().await {
        result?;
    }

    Ok(())
}

#[derive(Deserialize, Serialize, Debug, Clone)]
struct HealthResponse {
    status: bool,
}

async fn health() -> Result<AppJson<HealthResponse>, ApiError> {
    Ok(AppJson(HealthResponse { status: true }))
}

async fn create_project(
    State(state): State<AppState>,
    Json(request): Json<CreateProjectRequest>,
) -> Result<AppJson<CreateProjectResponse>, ApiError> {
    Ok(AppJson(state.app.create_project(request).await?))
}
