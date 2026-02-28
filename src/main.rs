use {
    axum::{
        Router,
        extract::DefaultBodyLimit,
        routing::{get, post},
    },
    sqlx::postgres::PgPoolOptions,
    std::{env, time::Duration},
    tokio::signal,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let stripe_webhook_secret =
        env::var("STRIPE_WEBHOOK_SECRET").expect("STRIPE_WEBHOOK_SECRET must be set");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&database_url)
        .await
        .expect("failed to connect to database");

    let state = fin_sync::AppState {
        pool,
        stripe_webhook_secret: stripe_webhook_secret.into(),
    };

    let app = Router::new()
        .route("/", get(|| async { "ok" }))
        .route(
            "/webhook",
            post(fin_sync::adapters::stripe::stripe_webhook_handler),
        )
        .layer(DefaultBodyLimit::max(64 * 1024)) // 64 KB — Stripe events are typically <20 KB
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("listening on 0.0.0.0:3000");
    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await
        .unwrap();
}

async fn shutdown_signal() {
    let ctrl_c = async {
        signal::ctrl_c().await.expect("failed to listen for ctrl+c");
    };

    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to listen for SIGTERM")
            .recv()
            .await;
    };

    tokio::select! {
        _ = ctrl_c => tracing::info!("received ctrl+c, shutting down"),
        _ = terminate => tracing::info!("received SIGTERM, shutting down"),
    }
}
