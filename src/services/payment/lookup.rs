use sqlx::PgPool;

use crate::{
    domain::{
        error::PipelineError,
        id::ExternalId,
        payment::{PaymentFilters, PaymentView},
    },
    infra::postgres::payment_repo,
};

pub async fn get_payment_by_id(
    pool: &PgPool,
    id: ExternalId,
) -> Result<Option<PaymentView>, PipelineError> {
    payment_repo::get_payment_by_id(pool, id).await
}

pub async fn get_payment_list(
    pool: &PgPool,
    mut filters: PaymentFilters,
) -> Result<Vec<PaymentView>, PipelineError> {
    filters.limit = Some(filters.limit.unwrap_or(20).min(100));
    if let Some(exact) = filters.amount {
        filters.amount_min = Some(exact);
        filters.amount_max = Some(exact);
    }
    payment_repo::get_list_payments(pool, filters).await
}
