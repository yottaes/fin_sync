use {
    axum::{Json, response::IntoResponse},
    derive_more::Display,
    serde::{self, Deserialize},
};

#[derive(Debug, Deserialize, Display)]
#[display(
    "id: {id}\nevent_type: {event_type}\ncreated: {created}\nlivemode: {livemode}\ndata: {data}"
)]
pub struct StripeEvent {
    pub id: String, // evt_xxx — idempotency key
    #[serde(rename = "type")]
    pub event_type: StripeEventType,
    pub created: i64,   // unix timestamp
    pub livemode: bool, // test vs prod
    pub data: StripeEventData,
}

#[derive(Debug, Deserialize, Display)]
pub struct StripeEventData {
    pub object: serde_json::Value, // типизируем по event_type
}

#[derive(Debug, Deserialize, Display)]
pub enum StripeEventType {
    #[serde(rename = "payment_intent.created")]
    PaymentIntentCreated,

    #[serde(rename = "payment_intent.succeeded")]
    PaymentIntentSucceeded,

    #[serde(rename = "charge.succeeded")]
    ChargeSucceeded,

    #[serde(rename = "charge.updated")]
    ChargeUpdated,

    #[serde(other)]
    Unknown,
}

pub async fn stripe_webhook_handler(Json(event): Json<StripeEvent>) -> impl IntoResponse {
    // use StripeEventType::*;
    // let val = match event.event_type {
    //     PaymentIntentCreated => "created",
    //     PaymentIntentSucceeded => "payment succeeded",
    //     ChargeSucceeded => "charge succeeded",
    //     ChargeUpdated => "charge updated",
    //     _ => {
    //         println!("Unknown Stripe event type!");
    //         " "
    //     }
    // };
    //
    println!("{}", event);

    "ok"
}
