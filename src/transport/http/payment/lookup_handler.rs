use axum::{
    Json,
    extract::{Path, Query, State},
};

use crate::{
    AppState,
    domain::{
        id::ExternalId,
        payment::{PaymentFilters, PaymentView},
    },
    services::payment::lookup::{get_payment_by_id, get_payment_list},
    transport::http::errors::ApiError,
};

pub async fn payment_by_id(
    State(state): State<AppState>,
    Path(id): Path<ExternalId>,
) -> Result<Json<PaymentView>, ApiError> {
    let payment = get_payment_by_id(&state.pool, id)
        .await?
        .ok_or_else(|| ApiError::not_found("payment not found"))?;

    Ok(Json(payment))
}

pub async fn payment_list(
    State(state): State<AppState>,
    Query(filters): Query<PaymentFilters>,
) -> Result<Json<Vec<PaymentView>>, ApiError> {
    let payments = get_payment_list(&state.pool, filters).await?;
    Ok(Json(payments))
}
