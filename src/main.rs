use {
    axum::{
        Router,
        routing::{get, post},
    },
    sqlx::postgres::PgPool,
    std::env,
};

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    dotenvy::dotenv().ok();
    let database_url = env::var("DATABASE_URL").expect("DATABASE_URL must be set");
    let stripe_webhook_secret =
        env::var("STRIPE_WEBHOOK_SECRET").expect("STRIPE_WEBHOOK_SECRET must be set");

    let pool = PgPool::connect(&database_url)
        .await
        .expect("failed to connect to database");

    let state = frg::AppState {
        pool,
        stripe_webhook_secret,
    };

    let app = Router::new()
        .route("/", get(|| async { "Hello world!" }))
        .route(
            "/webhook",
            post(frg::adapters::stripe::stripe_webhook_handler),
        )
        .with_state(state);

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}
