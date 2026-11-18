//! Domain error type and HTTP response wrappers.
//!
//! GET handlers preserve the original contract: `NotFound` and `BadRequest`
//! both become HTTP 404. POST handlers turn both into HTTP 400. The error body
//! is always `{"error": "..."}`.

use axum::extract::rejection::JsonRejection;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde_json::json;

/// Application-level failure with a user-facing message.
pub enum AppError {
    NotFound(String),
    BadRequest(String),
    Internal(String),
}

impl std::fmt::Debug for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(s) => write!(f, "NotFound({s})"),
            AppError::BadRequest(s) => write!(f, "BadRequest({s})"),
            AppError::Internal(s) => write!(f, "Internal({s})"),
        }
    }
}

impl std::fmt::Display for AppError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AppError::NotFound(s) => write!(f, "not found: {s}"),
            AppError::BadRequest(s) => write!(f, "bad request: {s}"),
            AppError::Internal(s) => write!(f, "internal: {s}"),
        }
    }
}

impl std::error::Error for AppError {}

impl From<std::io::Error> for AppError {
    fn from(e: std::io::Error) -> Self {
        if e.kind() == std::io::ErrorKind::NotFound {
            AppError::NotFound(e.to_string())
        } else {
            AppError::Internal(e.to_string())
        }
    }
}

fn status_for_get(e: &AppError) -> StatusCode {
    match e {
        AppError::NotFound(_) | AppError::BadRequest(_) => StatusCode::NOT_FOUND,
        AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn status_for_post(e: &AppError) -> StatusCode {
    match e {
        AppError::NotFound(_) | AppError::BadRequest(_) => StatusCode::BAD_REQUEST,
        AppError::Internal(_) => StatusCode::INTERNAL_SERVER_ERROR,
    }
}

fn error_body(e: &AppError) -> String {
    let msg = match e {
        AppError::NotFound(m) | AppError::BadRequest(m) | AppError::Internal(m) => m,
    };
    json!({ "error": msg }).to_string()
}

/// Error wrapper for GET routes (NotFound/BadRequest -> 404).
pub struct GetError(pub AppError);

impl IntoResponse for GetError {
    fn into_response(self) -> Response {
        let code = status_for_get(&self.0);
        (code, error_body(&self.0)).into_response()
    }
}

/// Error wrapper for POST routes (NotFound/BadRequest -> 400).
pub struct PostError(pub AppError);

impl IntoResponse for PostError {
    fn into_response(self) -> Response {
        let code = status_for_post(&self.0);
        (code, error_body(&self.0)).into_response()
    }
}

impl From<JsonRejection> for PostError {
    fn from(e: JsonRejection) -> Self {
        PostError(AppError::BadRequest(e.body_text()))
    }
}

impl From<std::io::Error> for PostError {
    fn from(e: std::io::Error) -> Self { PostError(e.into()) }
}

impl From<std::io::Error> for GetError {
    fn from(e: std::io::Error) -> Self { GetError(e.into()) }
}
