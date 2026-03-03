use {
    fin_sync::{
        adapters::stripe::client::StripeProvider,
        services::worker::{run_reaper, run_worker},
        transport::http::router,
    },
    sqlx::postgres::PgPoolOptions,
    std::{env, sync::Arc, time::Duration},
    tokio::signal,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let stripe_webhook_secret =
        env::var("STRIPE_WEBHOOK_SECRET").expect("STRIPE_WEBHOOK_SECRET must be set");
    let stripe_secret_key = env::var("STRIPE_SECRET_KEY").expect("STRIPE_SECRET_KEY must be set");

    let pool = PgPoolOptions::new()
        .max_connections(20)
        .acquire_timeout(Duration::from_secs(3))
        .connect(&database_url)
        .await
        .expect("failed to connect to database");

    let provider = Arc::new(StripeProvider::new(&stripe_secret_key));

    let state = fin_sync::AppState {
        pool,
        stripe_webhook_secret: stripe_webhook_secret.into(),
        provider,
    };

    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);

    tokio::spawn(run_worker(
        state.pool.clone(),
        state.provider.clone(),
        shutdown_rx.clone(),
    ));
    tokio::spawn(run_reaper(state.pool.clone(), shutdown_rx));

    let app = router::build(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    tracing::info!("listening on 0.0.0.0:3000");
    axum::serve(listener, app)
        .with_graceful_shutdown(async move {
            shutdown_signal().await;
            let _ = shutdown_tx.send(true);
        })
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
