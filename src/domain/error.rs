use thiserror::Error;

#[derive(Debug, Error)]
pub enum PipelineError {
    #[error("validation: {0}")]
    Validation(String),

    #[error("database: {0}")]
    Database(#[from] sqlx::Error),

    #[error("serialization: {0}")]
    Serialization(#[from] serde_json::Error),

    #[error("webhook signature: {0}")]
    WebhookSignature(String),
}
