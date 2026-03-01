pub mod adapters;
pub mod domain;
pub mod infra;
pub mod services;
pub mod transport;

use std::sync::Arc;

use domain::provider::PaymentProvider;

#[derive(Clone)]
pub struct AppState {
    pub pool: sqlx::PgPool,
    pub stripe_webhook_secret: Arc<str>,
    pub provider: Arc<dyn PaymentProvider>,
}
