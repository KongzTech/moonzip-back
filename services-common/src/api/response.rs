use std::{collections::BTreeMap, io::ErrorKind};

use axum::{
    extract::{multipart::MultipartError, rejection::JsonRejection, FromRequest},
    response::{IntoResponse, Response},
};

use reqwest::StatusCode;
use serde::{Deserialize, Serialize};

use utoipa::{
    openapi::{RefOr, Response as UtoipaResponse, ResponseBuilder, ResponsesBuilder},
    IntoResponses, ToSchema,
};

#[derive(FromRequest)]
#[from_request(via(axum::Json), rejection(ApiError))]
pub struct AppJson<T>(pub T);

impl<T> IntoResponse for AppJson<T>
where
    axum::Json<T>: IntoResponse,
{
    fn into_response(self) -> Response {
        axum::Json(self.0).into_response()
    }
}

#[derive(thiserror::Error, Debug)]
pub enum ApiError {
    /// Internal error occurred, message left unspecified.
    #[error("internal error occurred: {}", .0)]
    Internal(#[from] anyhow::Error),
    /// Passed json request is malformed.
    #[error("json request is malformed: {}", .0)]
    JsonRejection(JsonRejection),
    /// Request is logically invalid - check e.g. params matching.
    #[error("request is invalid: {}", .0)]
    InvalidRequest(anyhow::Error),
    #[error("captcha is invalid: {}", .0)]
    InvalidCaptcha(anyhow::Error),

    #[error("username is empty: {}", .0)]
    EmptyUsername(anyhow::Error),

    #[error("wallet address is empty: {}", .0)]
    EmptyWalletAddress(anyhow::Error),

    #[error("username existed: {}", .0)]
    ExistedUsername(anyhow::Error),

    #[error("Not found user: {}", .0)]
    NotFoundUser(anyhow::Error),

    #[error("Invalid username format: {}", .0)]
    InvalidUsernameFormat(anyhow::Error),

    #[error("NFT doesn't belong to user")]
    NFTNotBelong2User(anyhow::Error),
}

impl ApiError {
    fn code(&self) -> i16 {
        match self {
            ApiError::Internal(_) => 1,
            ApiError::JsonRejection(_) => 2,
            ApiError::InvalidRequest(_) => 3,
            ApiError::InvalidCaptcha(_) => 4,
            ApiError::EmptyUsername(_) => 10,
            ApiError::EmptyWalletAddress(_) => 11,
            ApiError::ExistedUsername(_) => 12,
            ApiError::NotFoundUser(_) => 13,
            ApiError::InvalidUsernameFormat(_) => 14,
            ApiError::NFTNotBelong2User(_) => 15,
        }
    }
}

impl From<MultipartError> for ApiError {
    fn from(err: MultipartError) -> Self {
        ApiError::InvalidRequest(anyhow::anyhow!(err))
    }
}

impl From<ApiError> for std::io::Error {
    fn from(value: ApiError) -> Self {
        Self::new(ErrorKind::InvalidInput, format!("{value}"))
    }
}

#[derive(Deserialize, Serialize, Debug, Clone, ToSchema, PartialEq, Eq)]
pub struct ErrorResponse {
    pub message: String,
    pub code: i16,
}

// implementing this allows to reuse response definition across all handlers.
impl IntoResponses for ErrorResponse {
    fn responses() -> BTreeMap<String, RefOr<UtoipaResponse>> {
        ResponsesBuilder::new()
            .response(
                "4XX",
                ResponseBuilder::new().description("Logical error due to user input"),
            )
            .response(
                "5XX",
                ResponseBuilder::new().description("Internal server error, contact support"),
            )
            .build()
            .into()
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let code = self.code();

        let (status, message) = match self {
            ApiError::Internal(err) => {
                tracing::error!("internal error while handling API request: {err:?}");

                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal server error".to_owned(),
                )
            }
            ApiError::JsonRejection(rejection) => (rejection.status(), rejection.body_text()),
            ApiError::InvalidRequest(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::InvalidCaptcha(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::EmptyUsername(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::EmptyWalletAddress(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::ExistedUsername(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::NotFoundUser(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::InvalidUsernameFormat(err) => (StatusCode::BAD_REQUEST, err.to_string()),
            ApiError::NFTNotBelong2User(err) => (StatusCode::BAD_REQUEST, err.to_string()),
        };

        (status, AppJson(ErrorResponse { message, code })).into_response()
    }
}
