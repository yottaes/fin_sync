use {
    crate::domain::audit::NewAuditEntry,
    crate::domain::error::PipelineError,
    crate::domain::payment::{
        NewPayment, NewPaymentParams, PassthroughEvent, PaymentAction, ProcessResult,
        WebhookTrigger,
    },
    crate::domain::provider::PaymentProvider,
    crate::infra::postgres::audit_repo::insert_audit_entry,
    crate::infra::postgres::payment_repo,
    sqlx::PgPool,
    uuid::Uuid,
};

/// Process a payment event: dedup, advisory lock, then insert or update
/// with state machine validation.
pub async fn process_payment_event(
    pool: &PgPool,
    payment: &NewPayment,
    actor: &str,
) -> Result<ProcessResult, PipelineError> {
    let mut tx = pool.begin().await?;

    sqlx::query!("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;

    // Serialize all processing for this external_id.
    sqlx::query!(
        "SELECT pg_advisory_xact_lock(hashtext($1))",
        payment.external_id()
    )
    .execute(&mut *tx)
    .await?;

    // Dedup: record the Stripe event. If already seen, bail early.
    let is_new = payment_repo::insert_provider_event(
        &mut tx,
        payment.last_event_id(),
        payment.external_id(),
        payment.event_type(),
        payment.provider_ts(),
        payment.raw_event(),
    )
    .await?;

    if !is_new {
        tx.commit().await?;
        return Ok(ProcessResult::Duplicate);
    }

    let existing = payment_repo::get_existing_payment(&mut tx, payment.external_id()).await?;

    match existing {
        None => {
            payment_repo::insert_payment(&mut tx, payment).await?;
            let audit = payment.audit_entry(actor, "created");
            insert_audit_entry(&mut tx, &audit).await?;
            tx.commit().await?;
            Ok(ProcessResult::Created(payment.id()))
        }
        Some(existing) => {
            let id = existing.id;
            let action = existing.decide(payment);

            match action {
                PaymentAction::SameStatus => {
                    payment_repo::touch_event_with_ts(
                        &mut tx,
                        id,
                        payment.last_event_id(),
                        payment.provider_ts(),
                    )
                    .await?;
                    tx.commit().await?;
                    Ok(ProcessResult::Stale(id))
                }
                PaymentAction::LogAnomaly { current } => {
                    let mut audit = payment.audit_entry(actor, "event_received");
                    audit.detail = serde_json::json!({
                        "event_type": payment.event_type(),
                        "current_status": current.as_str(),
                        "incoming_status": payment.status().as_str(),
                        "anomaly": true,
                    });
                    audit.entity_id = Some(id);
                    insert_audit_entry(&mut tx, &audit).await?;

                    payment_repo::touch_event_with_ts(
                        &mut tx,
                        id,
                        payment.last_event_id(),
                        payment.provider_ts(),
                    )
                    .await?;
                    tx.commit().await?;

                    tracing::warn!(
                        external_id = %payment.external_id(),
                        from = %current,
                        to = %payment.status(),
                        "invalid status transition, logged as anomaly"
                    );
                    Ok(ProcessResult::Anomaly(id))
                }
                PaymentAction::Advance { old_status } => {
                    payment_repo::update_payment_status(&mut tx, id, payment).await?;

                    let mut audit = payment.audit_entry(actor, "status_changed");
                    audit.detail = serde_json::json!({
                        "event_type": payment.event_type(),
                        "old_status": old_status.as_str(),
                        "new_status": payment.status().as_str(),
                    });
                    audit.entity_id = Some(id);
                    insert_audit_entry(&mut tx, &audit).await?;
                    tx.commit().await?;
                    Ok(ProcessResult::Updated(id))
                }
            }
        }
    }
}

/// Top-level orchestrator: webhook delivers a trigger, we fetch current state
/// from the provider API, then run the pipeline.
pub async fn process_webhook(
    pool: &PgPool,
    provider: &dyn PaymentProvider,
    trigger: WebhookTrigger,
    actor: &str,
) -> Result<ProcessResult, PipelineError> {
    match trigger {
        WebhookTrigger::Passthrough(event) => {
            let is_new = handle_passthrough(pool, &event).await?;
            if is_new {
                Ok(ProcessResult::Logged)
            } else {
                Ok(ProcessResult::Duplicate)
            }
        }
        WebhookTrigger::Payment {
            event_id,
            event_type,
            external_id,
            raw_event,
            provider_ts,
        } => {
            let fetched = provider.fetch_payment(&external_id).await?;
            let payment = NewPayment::new(NewPaymentParams {
                external_id: fetched.external_id,
                source: "stripe".into(),
                event_type,
                direction: fetched.direction,
                money: fetched.money,
                status: fetched.status,
                metadata: fetched.metadata,
                raw_event,
                last_event_id: event_id,
                parent_external_id: fetched.parent_external_id,
                provider_ts,
            });
            process_payment_event(pool, &payment, actor).await
        }
    }
}

/// Log an audit entry for events we don't upsert (charges, unknown).
pub async fn handle_passthrough(
    pool: &PgPool,
    event: &PassthroughEvent,
) -> Result<bool, PipelineError> {
    let mut tx = pool.begin().await?;

    let object_id = event
        .external_id
        .as_ref()
        .map(|id| id.as_str())
        .unwrap_or("");
    let is_new = payment_repo::insert_provider_event(
        &mut tx,
        event.event_id.as_str(),
        object_id,
        &event.event_type,
        event.provider_ts,
        &event.raw_payload,
    )
    .await?;

    if !is_new {
        tx.commit().await?;
        return Ok(false);
    }

    let entity_id = match &event.external_id {
        Some(eid) => payment_repo::find_payment_id(&mut tx, eid.as_str()).await?,
        None => None,
    };

    let audit = NewAuditEntry {
        id: Uuid::now_v7(),
        entity_type: "payment".to_string(),
        entity_id,
        external_id: event.external_id.as_ref().map(|id| id.as_str().to_string()),
        event_id: event.event_id.as_str().to_string(),
        action: "event_received".to_string(),
        actor: event.actor.clone(),
        detail: serde_json::json!({
            "event_type": event.event_type,
            "passthrough": true,
        }),
    };

    insert_audit_entry(&mut tx, &audit).await?;
    tx.commit().await?;
    Ok(true)
}
