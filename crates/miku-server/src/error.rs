use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};

pub(crate) type ServerResult<T> = Result<T, ServerError>;

#[derive(Debug)]
pub(crate) struct ServerError(miku_core::MikuError);

impl From<miku_core::MikuError> for ServerError {
    fn from(error: miku_core::MikuError) -> Self {
        Self(error)
    }
}

impl IntoResponse for ServerError {
    fn into_response(self) -> Response {
        let status = match self.0 {
            miku_core::MikuError::NotFound(_) => StatusCode::NOT_FOUND,
            miku_core::MikuError::Config(_) => StatusCode::BAD_REQUEST,
            _ => StatusCode::INTERNAL_SERVER_ERROR,
        };
        tracing::error!(status = %status, error = %self.0, "request failed");
        (
            status,
            Json(serde_json::json!({"error": self.0.to_string()})),
        )
            .into_response()
    }
}
