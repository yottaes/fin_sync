use crate::domain::error::PipelineError;
use axum::{
    Json,
    http::StatusCode,
    response::{IntoResponse, Response},
};

// Обертка (Newtype) для нашей доменной ошибки, чтобы реализовать для нее трейт Axum
pub struct ApiError(pub PipelineError);

// Реализуем автоматическую конвертацию PipelineError -> ApiError
impl From<PipelineError> for ApiError {
    fn from(err: PipelineError) -> Self {
        Self(err)
    }
}

// Вся логика HTTP-ответов теперь живет в слое адаптеров
impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self.0 {
            PipelineError::Validation(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                msg.clone(),
            ),
            PipelineError::WebhookSignature(_) => (
                StatusCode::BAD_REQUEST,
                "webhook_error",
                "invalid webhook signature".to_string(),
            ),
            PipelineError::Database(err) => {
                tracing::error!("database error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal error".to_string(),
                )
            }
            PipelineError::Serialization(err) => {
                tracing::error!("serialization error: {err}");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "internal_error",
                    "internal error".to_string(),
                )
            }
        };

        let body = serde_json::json!({
            "error_code": error_code,
            "message": message,
        });

        (status, Json(body)).into_response()
    }
}
