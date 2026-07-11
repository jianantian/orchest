//! Application error type implementing `IntoResponse` for axum.

use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use thiserror::Error;

/// Unified error type for the music-gift demo.
#[derive(Debug, Error)]
pub enum AppError {
    #[error("database error: {0}")]
    Database(String),

    #[error("not found: {0}")]
    NotFound(String),

    #[error("bad request: {0}")]
    BadRequest(String),

    #[error("configuration error: {0}")]
    Config(String),

    #[error("LLM error: {0}")]
    Llm(String),

    #[error("music generation error: {0}")]
    Gen(String),

    #[error("I/O error: {0}")]
    Io(#[from] std::io::Error),

    #[error("serialization error: {0}")]
    Serialize(#[from] serde_json::Error),

    #[error("base64 decode error: {0}")]
    Base64(#[from] base64::DecodeError),
}

#[derive(Serialize)]
struct ErrorResponse {
    error: String,
}

impl IntoResponse for AppError {
    fn into_response(self) -> Response {
        let status = match &self {
            Self::NotFound(_) => StatusCode::NOT_FOUND,
            Self::BadRequest(_) => StatusCode::BAD_REQUEST,
            Self::Config(_) => StatusCode::INTERNAL_SERVER_ERROR,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let body = axum::Json(ErrorResponse {
            error: self.to_string(),
        });
        (status, body).into_response()
    }
}

impl From<rusqlite::Error> for AppError {
    fn from(e: rusqlite::Error) -> Self {
        Self::Database(e.to_string())
    }
}

impl From<orchest_protocol::ProtocolError> for AppError {
    fn from(e: orchest_protocol::ProtocolError) -> Self {
        Self::Gen(e.to_string())
    }
}

impl From<orchest_protocol::ModelError> for AppError {
    fn from(e: orchest_protocol::ModelError) -> Self {
        Self::Llm(e.to_string())
    }
}

pub type AppResult<T> = Result<T, AppError>;
