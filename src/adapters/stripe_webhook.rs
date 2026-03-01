use {
    crate::{
        AppState,
        domain::{
            error::PipelineError,
            id::{EventId, ExternalId},
            payment::{PassthroughEvent, ProcessResult, WebhookTrigger},
        },
        services::payment_pipeline::process_webhook,
        transport::http::errors::ApiError,
    },
    axum::{Json, extract::State, http::HeaderMap},
};

#[tracing::instrument(
    name = "webhook",
    skip_all,
    fields(event_id = tracing::field::Empty, event_type = tracing::field::Empty)
)]
pub async fn wh_handler(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: String,
) -> Result<Json<serde_json::Value>, ApiError> {
    let sig = headers
        .get("Stripe-Signature")
        .and_then(|v| v.to_str().ok())
        .ok_or_else(|| PipelineError::WebhookSignature("missing Stripe-Signature header".into()))?;

    let event = stripe::Webhook::construct_event(&body, sig, &state.stripe_webhook_secret)
        .map_err(|e| PipelineError::WebhookSignature(e.to_string()))?;

    let event_id = event.id.to_string();
    let stripe_created = event.created;
    let raw_event: serde_json::Value = serde_json::from_str(&body).map_err(PipelineError::from)?;
    let event_type = raw_event
        .get("type")
        .and_then(|v| v.as_str())
        .unwrap_or("unknown")
        .to_string();

    tracing::Span::current()
        .record("event_id", tracing::field::display(&event_id))
        .record("event_type", tracing::field::display(&event_type));

    let trigger = match event.data.object {
        stripe::EventObject::PaymentIntent(ref pi) => {
            let external_id = match ExternalId::new(pi.id.to_string()) {
                Ok(id) => id,
                Err(PipelineError::Validation(msg)) => {
                    tracing::warn!(event_type = %event_type, "skipping invalid PI id: {msg}");
                    return Ok(Json(serde_json::json!({"status": "ignored_invalid_data"})));
                }
                Err(e) => return Err(e.into()),
            };
            WebhookTrigger::Payment {
                event_id: EventId::new(event_id.clone())?,
                event_type: event_type.clone(),
                external_id,
                raw_event,
                provider_ts: stripe_created,
            }
        }
        stripe::EventObject::Refund(ref refund) => {
            let external_id = match ExternalId::new(refund.id.to_string()) {
                Ok(id) => id,
                Err(PipelineError::Validation(msg)) => {
                    tracing::warn!(event_type = %event_type, "skipping invalid refund id: {msg}");
                    return Ok(Json(serde_json::json!({"status": "ignored_invalid_data"})));
                }
                Err(e) => return Err(e.into()),
            };
            WebhookTrigger::Payment {
                event_id: EventId::new(event_id.clone())?,
                event_type: event_type.clone(),
                external_id,
                raw_event,
                provider_ts: stripe_created,
            }
        }
        stripe::EventObject::Charge(ref charge) => {
            let pi_id = charge
                .payment_intent
                .as_ref()
                .map(|e| match e {
                    stripe::Expandable::Id(id) => ExternalId::new(id.to_string()),
                    stripe::Expandable::Object(pi) => ExternalId::new(pi.id.to_string()),
                })
                .transpose()?;
            WebhookTrigger::Passthrough(PassthroughEvent {
                external_id: pi_id,
                event_id: EventId::new(event_id.clone())?,
                event_type: event_type.clone(),
                provider_ts: stripe_created,
                raw_payload: raw_event,
                actor: "webhook:stripe".into(),
            })
        }
        _ => WebhookTrigger::Passthrough(PassthroughEvent {
            external_id: None,
            event_id: EventId::new(event_id.clone())?,
            event_type: event_type.clone(),
            provider_ts: stripe_created,
            raw_payload: raw_event,
            actor: "webhook:stripe".into(),
        }),
    };

    match process_webhook(&state.pool, &*state.provider, trigger, "webhook:stripe").await? {
        ProcessResult::Created(id) => {
            tracing::info!(payment_id = %id, "payment created");
            Ok(Json(serde_json::json!({"status": "created"})))
        }
        ProcessResult::Updated(id) => {
            tracing::info!(payment_id = %id, "payment updated");
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
