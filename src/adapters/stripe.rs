use {
    crate::{
        AppState,
        adapters::api_errors::ApiError,
        domain::{
            error::PipelineError,
            money::{Currency, Money, MoneyAmount},
            payment::{NewPayment, PaymentDirection, PaymentStatus},
        },
        infra::postgres::payment_repo::{ProcessResult, log_passthrough_event, process_payment_event},
    },
    axum::{Json, extract::State, http::HeaderMap},
    uuid::Uuid,
};

fn convert_currency(c: stripe::Currency) -> Result<Currency, PipelineError> {
    match c {
        stripe::Currency::USD => Ok(Currency::Usd),
        stripe::Currency::EUR => Ok(Currency::Eur),
        stripe::Currency::GBP => Ok(Currency::Gbp),
        stripe::Currency::JPY => Ok(Currency::Jpy),
        other => Err(PipelineError::Validation(format!(
            "unsupported currency: {other:?}"
        ))),
    }
}

fn convert_amount(amount: i64) -> Result<MoneyAmount, PipelineError> {
    let amount: u64 = amount
        .try_into()
        .map_err(|_| PipelineError::Validation("negative amount".into()))?;
    Ok(MoneyAmount::new(amount))
}

fn convert_pi_status(status: stripe::PaymentIntentStatus) -> PaymentStatus {
    #[allow(unreachable_patterns)]
    match status {
        stripe::PaymentIntentStatus::Succeeded => PaymentStatus::Succeeded,
        stripe::PaymentIntentStatus::Canceled => PaymentStatus::Failed,
        stripe::PaymentIntentStatus::Processing
        | stripe::PaymentIntentStatus::RequiresAction
        | stripe::PaymentIntentStatus::RequiresCapture
        | stripe::PaymentIntentStatus::RequiresConfirmation
        | stripe::PaymentIntentStatus::RequiresPaymentMethod => PaymentStatus::Pending,
        other => {
            tracing::warn!("unknown PaymentIntentStatus: {other:?}, defaulting to Pending");
            PaymentStatus::Pending
        }
    }
}

fn convert_refund_status(status: Option<&str>) -> PaymentStatus {
    match status {
        Some("succeeded") => PaymentStatus::Refunded,
        Some("failed") | Some("canceled") => PaymentStatus::Failed,
        _ => PaymentStatus::Pending,
    }
}

fn payment_from_pi(
    pi: &stripe::PaymentIntent,
    event_id: &str,
    event_type: &str,
    raw_event: serde_json::Value,
    stripe_created: i64,
) -> Result<NewPayment, PipelineError> {
    let currency = convert_currency(pi.currency)?;
    let amount = convert_amount(pi.amount)?;
    let status = convert_pi_status(pi.status);
    let metadata = serde_json::to_value(&pi.metadata)?;

    Ok(NewPayment::new(
        Uuid::now_v7(),
        pi.id.to_string(),
        "stripe".to_string(),
        event_type.to_string(),
        PaymentDirection::Inbound,
        Money::new(amount, currency),
        status,
        metadata,
        raw_event,
        event_id.to_string(),
        None,
        stripe_created,
    ))
}

fn payment_from_refund(
    refund: &stripe::Refund,
    event_id: &str,
    event_type: &str,
    raw_event: serde_json::Value,
    stripe_created: i64,
) -> Result<NewPayment, PipelineError> {
    let currency = convert_currency(refund.currency)?;
    let amount = convert_amount(refund.amount)?;
    let status = convert_refund_status(refund.status.as_deref());
    let metadata = refund
        .metadata
        .as_ref()
        .map(serde_json::to_value)
        .transpose()?
        .unwrap_or(serde_json::Value::Null);

    let parent_pi_id = refund.payment_intent.as_ref().map(|e| match e {
        stripe::Expandable::Id(id) => id.to_string(),
        stripe::Expandable::Object(pi) => pi.id.to_string(),
    });

    Ok(NewPayment::new(
        Uuid::now_v7(),
        refund.id.to_string(),
        "stripe".to_string(),
        event_type.to_string(),
        PaymentDirection::Outbound,
        Money::new(amount, currency),
        status,
        metadata,
        raw_event,
        event_id.to_string(),
        parent_pi_id,
        stripe_created,
    ))
}

pub async fn stripe_webhook_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sig = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| {
            PipelineError::WebhookSignature("missing Stripe-Signature header".into())
        })?;

    let event = stripe::Webhook::construct_event(&body, sig, &state.stripe_webhook_secret)
        .map_err(|e| PipelineError::WebhookSignature(e.to_string()))?;

    let event_id = event.id.to_string();
    let stripe_created = event.created;
    let raw_event: serde_json::Value =
        serde_json::from_str(&body).map_err(PipelineError::from)?;
    let event_type = raw_event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    let payment = match event.data.object {
        stripe::EventObject::PaymentIntent(ref pi) => {
            match payment_from_pi(pi, &event_id, &event_type, raw_event, stripe_created) {
                Ok(p) => p,
                Err(PipelineError::Validation(msg)) => {
                    tracing::warn!(event_type = %event_type, "skipping invalid PI data: {msg}");
                    return Ok(Json(serde_json::json!({"status": "ignored_invalid_data"})));
                }
                Err(e) => return Err(e.into()),
            }
        }
        stripe::EventObject::Refund(ref refund) => {
            match payment_from_refund(refund, &event_id, &event_type, raw_event, stripe_created) {
                Ok(p) => p,
                Err(PipelineError::Validation(msg)) => {
                    tracing::warn!(event_type = %event_type, "skipping invalid refund data: {msg}");
                    return Ok(Json(serde_json::json!({"status": "ignored_invalid_data"})));
                }
                Err(e) => return Err(e.into()),
            }
        }
        stripe::EventObject::Charge(ref charge) => {
            let pi_id = charge.payment_intent.as_ref().map(|e| match e {
                stripe::Expandable::Id(id) => id.to_string(),
                stripe::Expandable::Object(pi) => pi.id.to_string(),
            });
            let is_new = log_passthrough_event(
                &state.pool,
                pi_id.as_deref(),
                &event_id,
                &event_type,
                stripe_created,
                &raw_event,
            )
            .await?;
            let status = if is_new { "logged" } else { "duplicate" };
            tracing::info!(event_type = %event_type, status, "charge event");
            return Ok(Json(serde_json::json!({"status": status})));
        }
        _ => {
            let is_new = log_passthrough_event(
                &state.pool,
                None,
                &event_id,
                &event_type,
                stripe_created,
                &raw_event,
            )
            .await?;
            let status = if is_new { "logged" } else { "duplicate" };
            tracing::info!(event_type = %event_type, status, "unsupported event");
            return Ok(Json(serde_json::json!({"status": status})));
        }
    };

    match process_payment_event(&state.pool, &payment).await? {
        ProcessResult::Created(id) => {
            tracing::info!(payment_id = %id, direction = ?payment.direction(), "payment created");
            Ok(Json(serde_json::json!({"status": "created"})))
        }
        ProcessResult::Updated(id) => {
            tracing::info!(payment_id = %id, direction = ?payment.direction(), "payment updated");
            Ok(Json(serde_json::json!({"status": "updated"})))
        }
        ProcessResult::Stale(id) => {
            tracing::info!(payment_id = %id, event_type = %event_type, "stale event, skipped");
            Ok(Json(serde_json::json!({"status": "skipped"})))
        }
        ProcessResult::Duplicate => {
            tracing::info!(event_id = %event_id, "duplicate event, already processed");
            Ok(Json(serde_json::json!({"status": "duplicate"})))
        }
        ProcessResult::Anomaly(id) => {
            tracing::warn!(payment_id = %id, event_type = %event_type, "anomalous transition, logged");
            Ok(Json(serde_json::json!({"status": "anomaly"})))
        }
    }
}
