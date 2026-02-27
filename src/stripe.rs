use serde::{self, Deserialize};

#[derive(Debug, Deserialize)]
pub struct StripeEvent {
    #[serde(rename = "type")]
    pub event_type: StripeEventType,
}

#[derive(Debug, Deserialize)]
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

