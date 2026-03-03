use crate::domain::error::PipelineError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

/// HTTP error response. Not coupled to any specific domain error —
/// can represent 404, 422, 500, or anything else.
pub struct ApiError {
    status: StatusCode,
    code: &'static str,
    message: String,
}

impl ApiError {
    pub fn not_found(message: impl Into<String>) -> Self {
        Self {
            status: StatusCode::NOT_FOUND,
            code: "not_found",
            message: message.into(),
        }
    }
}

/// PipelineError → ApiError so `?` works in handlers.
impl From<PipelineError> for ApiError {
    fn from(err: PipelineError) -> Self {
        match err {
            PipelineError::Validation(msg) => {
                tracing::warn!("validation error: {msg}");
                Self {
                    status: StatusCode::UNPROCESSABLE_ENTITY,
                    code: "validation_error",
                    message: "request could not be processed".into(),
                }
            }
            PipelineError::WebhookSignature(_) => Self {
                status: StatusCode::BAD_REQUEST,
                code: "webhook_error",
                message: "invalid webhook signature".into(),
            },
            PipelineError::Database(err) => {
                tracing::error!("database error: {err}");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: "internal_error",
                    message: "internal error".into(),
                }
            }
            PipelineError::Serialization(err) => {
                tracing::error!("serialization error: {err}");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: "internal_error",
                    message: "internal error".into(),
                }
            }
            PipelineError::Provider(err) => {
                tracing::error!("provider error: {err}");
                Self {
                    status: StatusCode::INTERNAL_SERVER_ERROR,
                    code: "provider_error",
                    message: "internal error".into(),
                }
            }
        }
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let body = serde_json::json!({
            "error_code": self.code,
            "message": self.message,
        });
        (self.status, Json(body)).into_response()
    }
}
