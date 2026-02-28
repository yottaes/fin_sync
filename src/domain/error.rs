use {
    axum::Json,
    axum::http::StatusCode,
    axum::response::{IntoResponse, Response},
    thiserror::Error,
};

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("validation: {0}")]
    Validation(String),

    #[error("database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),
}

impl IntoResponse for PipelineError {
    fn into_response(self) -> Response {
        let (status, error_code, message) = match &self {
            PipelineError::Validation(msg) => (
                StatusCode::UNPROCESSABLE_ENTITY,
                "validation_error",
                msg.clone(),
            ),
            PipelineError::Database(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal error".to_string(),
            ),
            PipelineError::Serialization(_) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "internal_error",
                "internal error".to_string(),
            ),
        };

        let body = serde_json::json!({
            "error_code": error_code,
            "message": message,
        });

        (status, Json(body)).into_response()
    }
}
