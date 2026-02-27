use {
    axum::{
        Json, Router,
        response::IntoResponse,
        routing::{get, post},
    },
    frg::stripe::{StripeEvent, StripeEventType},
};

#[tokio::main]
async fn main() {
    let app = Router::new()
        .route("/", get(|| async { "Hello world!" }))
        .route("/webhook", post(echo));

    let listener = tokio::net::TcpListener::bind("0.0.0.0:3000").await.unwrap();
    axum::serve(listener, app).await.unwrap();
}

async fn echo(Json(event): Json<StripeEvent>) -> impl IntoResponse {
    use StripeEventType::*;
    let val = match event.event_type {
        PaymentIntentCreated => "created",
        PaymentIntentSucceeded => "payment succeeded",
        ChargeSucceeded => "charge succeeded",
        ChargeUpdated => "charge updated",
        _ => {
            println!("Unknown Stripe event type!");
            " "
        }
    };

    // println!("{}", val);

    // println!("==========  Stripe WebHook ==========");
    // println!("{:#?}", event);
    // println!("=====================================");

    "ok"
}
