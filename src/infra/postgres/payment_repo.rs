use {
    super::audit_repo::insert_audit_entry,
    crate::domain::error::PipelineError,
    crate::domain::payment::{NewPayment, PaymentStatus},
    sqlx::PgPool,
    uuid::Uuid,
};

#[derive(Debug)]
pub enum ProcessResult {
    /// New payment row inserted.
    Created(Uuid),
    /// Existing payment row updated (status advanced).
    Updated(Uuid),
    /// Event is older than what we've already processed — no state change.
    Stale(Uuid),
    /// Stripe event was already processed (duplicate delivery).
    Duplicate,
    /// Transition is not valid per state machine — logged as anomaly.
    Anomaly(Uuid),
}

/// Log an audit entry for events we don't upsert (charges, unknown).
/// Records the event in provider_events for dedup, then writes an audit row.
pub async fn log_passthrough_event(
    pool: &PgPool,
    external_id: Option<&str>,
    event_id: &str,
    event_type: &str,
    provider_ts: i64,
    raw_payload: &serde_json::Value,
) -> Result<bool, PipelineError> {
    let mut tx = pool.begin().await?;

    // Dedup: record event, bail if already seen.
    let inserted = sqlx::query_scalar::<_, bool>(
        r#"
        INSERT INTO provider_events (event_id, object_id, event_type, provider_ts, payload)
        VALUES ($1, COALESCE($2, ''), $3, $4, $5)
        ON CONFLICT (event_id) DO NOTHING
        RETURNING true
        "#,
    )
    .bind(event_id)
    .bind(external_id)
    .bind(event_type)
    .bind(provider_ts)
    .bind(raw_payload)
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        tx.commit().await?;
        return Ok(false); // duplicate
    }

    let entity_id = match external_id {
        Some(eid) => {
            let row: Option<(Uuid,)> =
                sqlx::query_as("SELECT id FROM payments WHERE external_id = $1")
                    .bind(eid)
                    .fetch_optional(&mut *tx)
                    .await?;
            row.map(|(id,)| id)
        }
        None => None,
    };

    let audit = crate::domain::audit::NewAuditEntry {
        id: Uuid::now_v7(),
        entity_type: "payment".to_string(),
        entity_id,
        external_id: external_id.map(|s| s.to_string()),
        event_id: event_id.to_string(),
        action: "event_received".to_string(),
        actor: "webhook:stripe".to_string(),
        detail: serde_json::json!({
            "event_type": event_type,
            "passthrough": true,
        }),
    };

    insert_audit_entry(&mut tx, &audit).await?;
    tx.commit().await?;
    Ok(true)
}

/// Process a payment event: dedup via event log, advisory lock on object,
/// then insert or update the payment row with state machine validation.
pub async fn process_payment_event(
    pool: &PgPool,
    payment: &NewPayment,
) -> Result<ProcessResult, PipelineError> {
    let mut tx = pool.begin().await?;

    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;

    // 1. Serialize all processing for this external_id.
    //    Advisory lock works even when the row doesn't exist yet —
    //    no gap lock issue, no insert race, no retry needed.
    sqlx::query("SELECT pg_advisory_xact_lock(hashtext($1))")
        .bind(payment.external_id())
        .execute(&mut *tx)
        .await?;

    // 2. Dedup: record the Stripe event. If already seen, bail early.
    let inserted = sqlx::query_scalar::<_, bool>(
        r#"
        INSERT INTO provider_events (event_id, object_id, event_type, provider_ts, payload)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (event_id) DO NOTHING
        RETURNING true
        "#,
    )
    .bind(payment.last_event_id())
    .bind(payment.external_id())
    .bind(payment.event_type())
    .bind(payment.provider_ts())
    .bind(payment.raw_event())
    .fetch_optional(&mut *tx)
    .await?;

    if inserted.is_none() {
        tx.commit().await?;
        return Ok(ProcessResult::Duplicate);
    }

    let pg_amount: i64 = payment
        .money()
        .amount()
        .cents()
        .try_into()
        .map_err(|_| PipelineError::Validation("amount exceeds storage capacity".into()))?;

    // 3. Get current state (no FOR UPDATE needed — advisory lock covers us).
    let existing: Option<(Uuid, String, i64)> = sqlx::query_as(
        "SELECT id, status, last_provider_ts FROM payments WHERE external_id = $1",
    )
    .bind(payment.external_id())
    .fetch_optional(&mut *tx)
    .await?;

    match existing {
        None => {
            sqlx::query(
                r#"
                INSERT INTO payments
                    (id, external_id, source, event_type, direction,
                     amount, currency, status, metadata, raw_event,
                     last_event_id, parent_external_id, last_provider_ts)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
                "#,
            )
            .bind(payment.id())
            .bind(payment.external_id())
            .bind(payment.source())
            .bind(payment.event_type())
            .bind(payment.direction().as_str())
            .bind(pg_amount)
            .bind(payment.money().currency().as_str())
            .bind(payment.status().as_str())
            .bind(payment.metadata())
            .bind(payment.raw_event())
            .bind(payment.last_event_id())
            .bind(payment.parent_external_id())
            .bind(payment.provider_ts())
            .execute(&mut *tx)
            .await?;

            let audit = payment.audit_entry("webhook:stripe", "created");
            insert_audit_entry(&mut tx, &audit).await?;
            tx.commit().await?;
            Ok(ProcessResult::Created(payment.id()))
        }
        Some((id, old_status_str, last_provider_ts)) => {
            let old_status = PaymentStatus::try_from(old_status_str.as_str())?;

            // Same status — nothing to change, just track that we saw this event.
            if *payment.status() == old_status {
                sqlx::query(
                    "UPDATE payments SET last_event_id = $1, last_provider_ts = GREATEST(last_provider_ts, $2), updated_at = now() WHERE id = $3",
                )
                .bind(payment.last_event_id())
                .bind(payment.provider_ts())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                return Ok(ProcessResult::Stale(id));
            }

            // Temporal check: is this event newer than what we've already processed?
            // Use strict < because Stripe events within the same second share
            // a timestamp — equal timestamps fall through to the state machine.
            if payment.provider_ts() < last_provider_ts {
                let mut audit = payment.audit_entry("webhook:stripe", "event_received");
                audit.detail = serde_json::json!({
                    "event_type": payment.event_type(),
                    "current_status": old_status_str,
                    "incoming_status": payment.status().as_str(),
                    "stale": true,
                });
                audit.entity_id = Some(id);
                insert_audit_entry(&mut tx, &audit).await?;

                sqlx::query(
                    "UPDATE payments SET last_event_id = $1, updated_at = now() WHERE id = $2",
                )
                .bind(payment.last_event_id())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;
                Ok(ProcessResult::Stale(id))
            }
            // State machine check: is this transition valid?
            else if !old_status.can_transition_to(payment.status()) {
                let mut audit = payment.audit_entry("webhook:stripe", "event_received");
                audit.detail = serde_json::json!({
                    "event_type": payment.event_type(),
                    "current_status": old_status_str,
                    "incoming_status": payment.status().as_str(),
                    "anomaly": true,
                });
                audit.entity_id = Some(id);
                insert_audit_entry(&mut tx, &audit).await?;

                // Still update last_event_id and timestamp so we don't reprocess.
                sqlx::query(
                    "UPDATE payments SET last_event_id = $1, last_provider_ts = $2, updated_at = now() WHERE id = $3",
                )
                .bind(payment.last_event_id())
                .bind(payment.provider_ts())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                tx.commit().await?;

                tracing::warn!(
                    external_id = %payment.external_id(),
                    from = %old_status,
                    to = %payment.status(),
                    "invalid status transition, logged as anomaly"
                );
                Ok(ProcessResult::Anomaly(id))
            } else {
                sqlx::query(
                    r#"
                    UPDATE payments
                    SET status = $1, event_type = $2, metadata = $3,
                        last_event_id = $4, last_provider_ts = $5, updated_at = now()
                    WHERE id = $6
                    "#,
                )
                .bind(payment.status().as_str())
                .bind(payment.event_type())
                .bind(payment.metadata())
                .bind(payment.last_event_id())
                .bind(payment.provider_ts())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                let mut audit = payment.audit_entry("webhook:stripe", "status_changed");
                audit.detail = serde_json::json!({
                    "event_type": payment.event_type(),
                    "old_status": old_status_str,
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
