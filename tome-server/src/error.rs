use axum::{Json, http::StatusCode, response::IntoResponse};
use serde_json::json;

pub type AppResult<T> = std::result::Result<T, AppError>;

#[derive(Debug)]
enum ErrorKind {
    NotFound,
    BadRequest,
    Internal,
}

pub struct AppError {
    kind: ErrorKind,
    source: anyhow::Error,
}

impl AppError {
    pub fn not_found(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::NotFound, source: anyhow::anyhow!(msg.into()) }
    }

    pub fn bad_request(msg: impl Into<String>) -> Self {
        Self { kind: ErrorKind::BadRequest, source: anyhow::anyhow!(msg.into()) }
    }
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        let status = match self.kind {
            ErrorKind::NotFound => StatusCode::NOT_FOUND,
            ErrorKind::BadRequest => StatusCode::BAD_REQUEST,
            ErrorKind::Internal => StatusCode::INTERNAL_SERVER_ERROR,
        };
        let msg = match self.kind {
            ErrorKind::Internal => {
                if cfg!(debug_assertions) {
                    format!("{:#}", self.source)
                } else {
                    "internal server error".to_owned()
                }
            }
            _ => self.source.to_string(),
        };
        (status, Json(json!({ "error": msg }))).into_response()
    }
}

impl<E: Into<anyhow::Error>> From<E> for AppError {
    fn from(e: E) -> Self {
        AppError { kind: ErrorKind::Internal, source: e.into() }
    }
}
