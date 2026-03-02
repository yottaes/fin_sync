use {
    crate::{
        AppState,
        domain::{
            error::PipelineError,
            id::{EventId, ExternalId},
            payment::{PassthroughEvent, PaymentTrigger, WebhookTrigger},
        },
        infra::postgres::job_repo,
        services::payment_pipeline::handle_passthrough,
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
            WebhookTrigger::Payment(PaymentTrigger {
                event_id: EventId::new(event_id.clone())?,
                event_type: event_type.clone(),
                external_id,
                raw_event,
                provider_ts: stripe_created,
            })
        }
        stripe::EventObject::Refund(ref refund) if !event_type.starts_with("charge.refund") => {
            let external_id = match ExternalId::new(refund.id.to_string()) {
                Ok(id) => id,
                Err(PipelineError::Validation(msg)) => {
                    tracing::warn!(event_type = %event_type, "skipping invalid refund id: {msg}");
                    return Ok(Json(serde_json::json!({"status": "ignored_invalid_data"})));
                }
                Err(e) => return Err(e.into()),
            };
            WebhookTrigger::Payment(PaymentTrigger {
                event_id: EventId::new(event_id.clone())?,
                event_type: event_type.clone(),
                external_id,
                raw_event,
                provider_ts: stripe_created,
            })
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

    match trigger {
        WebhookTrigger::Payment(t) => {
            let inserted = job_repo::enqueue(
                &state.pool,
                t.event_id.as_str(),
                t.external_id.as_str(),
                &t.event_type,
                t.provider_ts,
                &t.raw_event,
            )
            .await?;

            if inserted {
                tracing::info!("payment event enqueued for async processing");
                Ok(Json(serde_json::json!({"status": "accepted"})))
            } else {
                tracing::info!("duplicate event, already enqueued");
                Ok(Json(serde_json::json!({"status": "duplicate"})))
            }
        }
        WebhookTrigger::Passthrough(event) => {
            let is_new = handle_passthrough(&state.pool, &event).await?;
            if is_new {
                tracing::info!(event_type = %event_type, "passthrough event logged");
                Ok(Json(serde_json::json!({"status": "logged"})))
            } else {
                tracing::info!(event_id = %event_id, "duplicate event, already processed");
                Ok(Json(serde_json::json!({"status": "duplicate"})))
            }
        }
    }
}
