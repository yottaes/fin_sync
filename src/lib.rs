pub mod adapters;
pub mod domain;
pub mod infra;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub stripe_webhook_secret: String,
}
