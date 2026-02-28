use {
    crate::domain::error::PipelineError,
    crate::domain::money::{Currency, Money, MoneyAmount},
    crate::domain::payment::{NewPayment, PaymentDirection, PaymentStatus},
    crate::infra::postgres::payment_repo::{InsertResult, insert_payment},
    axum::{Json, extract::State},
    derive_more::Display,
    serde::{Deserialize, Serialize},
    sqlx::PgPool,
    uuid::Uuid,
};

#[derive(Debug, Deserialize, Serialize, Display)]
#[display(
    "id: {id}\nevent_type: {event_type}\ncreated: {created}\nlivemode: {livemode}\ndata: {data}"
)]
pub struct StripeEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: StripeEventType,
    pub created: i64,
    pub livemode: bool,
    pub data: StripeEventData,
}

#[derive(Debug, Deserialize, Serialize, Display)]
pub struct StripeEventData {
    pub object: serde_json::Value,
}

#[derive(Debug, Deserialize, Serialize, Display)]
pub enum StripeEventType {
    #[serde(rename = "payment_intent.created")]
    PaymentIntentCreated,

    #[serde(rename = "payment_intent.succeeded")]
    PaymentIntentSucceeded,

    #[serde(rename = "charge.succeeded")]
    ChargeSucceeded,

    #[serde(rename = "charge.updated")]
    ChargeUpdated,

    #[serde(rename = "refund.created")]
    RefundCreated,

    #[serde(rename = "refund.updated")]
    RefundUpdated,

    #[serde(other, rename = "unknown")]
    Unknown,
}

impl StripeEventType {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaymentIntentCreated => "payment_intent.created",
            Self::PaymentIntentSucceeded => "payment_intent.succeeded",
            Self::ChargeSucceeded => "charge.succeeded",
            Self::ChargeUpdated => "charge.updated",
            Self::RefundCreated => "refund.created",
            Self::RefundUpdated => "refund.updated",
            Self::Unknown => "unknown",
        }
    }
}

impl TryFrom<&StripeEvent> for NewPayment {
    type Error = PipelineError;

    fn try_from(event: &StripeEvent) -> Result<Self, Self::Error> {
        let direction = match event.event_type {
            StripeEventType::PaymentIntentSucceeded
            | StripeEventType::PaymentIntentCreated
            | StripeEventType::ChargeSucceeded
            | StripeEventType::ChargeUpdated => PaymentDirection::Inbound,

            StripeEventType::RefundCreated | StripeEventType::RefundUpdated => {
                PaymentDirection::Outbound
            }

            _ => {
                return Err(PipelineError::Validation(format!(
                    "ignored event type: {:?}",
                    event.event_type
                )));
            }
        };

        let obj = &event.data.object;

        let amount = obj
            .get("amount")
            .and_then(|v| v.as_u64())
            .ok_or_else(|| PipelineError::Validation("missing or invalid 'amount'".to_string()))?;

        let currency_str = obj
            .get("currency")
            .and_then(|v| v.as_str())
            .ok_or_else(|| {
                PipelineError::Validation("missing or invalid 'currency'".to_string())
            })?;

        let currency = Currency::try_from(currency_str).map_err(PipelineError::Validation)?;

        let money = Money::new(MoneyAmount::new(amount), currency);

        let status_str = obj
            .get("status")
            .and_then(|v| v.as_str())
            .unwrap_or("pending");

        let canonical_status = match status_str {
            "requires_payment_method"
            | "requires_confirmation"
            | "requires_action"
            | "processing"
            | "requires_capture" => "pending",
            "succeeded" if direction == PaymentDirection::Outbound => "refunded",
            other => other,
        };

        let status = PaymentStatus::try_from(canonical_status)?;

        let metadata = obj
            .get("metadata")
            .cloned()
            .unwrap_or(serde_json::Value::Null);

        let raw_event = serde_json::to_value(event)?;

        NewPayment::new(
            Uuid::now_v7(),
            event.id.clone(),
            "stripe".to_string(),
            event.event_type.as_str().to_string(),
            direction,
            money,
            status,
            metadata,
            raw_event,
        )
    }
}

pub async fn stripe_webhook_handler(
    State(pool): State<PgPool>,
    Json(event): Json<StripeEvent>,
) -> Result<Json<serde_json::Value>, PipelineError> {
    let payment = NewPayment::try_from(&event)
        .inspect_err(|e| tracing::warn!("failed to convert stripe event: {e}"))?;

    let audit = payment.audit_entry("webhook:stripe");

    match insert_payment(&pool, &payment, &audit).await? {
        InsertResult::Created(id) => {
            tracing::info!(payment_id = %id, direction = ?payment.direction(), "payment created");
            Ok(Json(serde_json::json!({"status": "created"})))
        }

        InsertResult::Duplicate => {
            tracing::info!(external_id = %event.id, "duplicate event");
            Ok(Json(serde_json::json!({"status": "duplicate"})))
        }
    }
}
