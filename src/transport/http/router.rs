use axum::{
    Router,
    extract::DefaultBodyLimit,
    routing::{get, post},
};
use std::time::Duration;
use tower_http::timeout::TimeoutLayer;

use crate::{
    AppState,
    adapters::stripe::webhook::wh_handler,
    transport::http::payment::lookup_handler::{payment_by_id, payment_list},
};

pub fn build(state: AppState) -> Router {
    Router::new()
        .route("/", get(|| async { "ok" }))
        .route("/webhook", post(wh_handler))
        .route("/payments/{id}", get(payment_by_id))
        .route("/payments", get(payment_list))
        .layer(DefaultBodyLimit::max(64 * 1024))
        .layer(TimeoutLayer::with_status_code(
            axum::http::StatusCode::REQUEST_TIMEOUT,
            Duration::from_secs(30),
        ))
        .with_state(state)
}
