use {
    super::audit_repo::insert_audit_entry,
    crate::domain::audit::NewAuditEntry,
    crate::domain::error::PipelineError,
    crate::domain::payment::{NewPayment, PaymentStatus},
    sqlx::PgPool,
    uuid::Uuid,
};

#[derive(Debug)]
pub enum UpsertResult {
    Created(Uuid),
    Updated(Uuid),
    Skipped(Uuid),
}

/// Log an audit entry for events we don't upsert (charges, unknown).
/// Links to the payment row via `external_id` if one exists.
pub async fn log_passthrough_event(
    pool: &PgPool,
    external_id: Option<&str>,
    event_id: &str,
    event_type: &str,
) -> Result<(), PipelineError> {
    let mut tx = pool.begin().await?;

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

    let audit = NewAuditEntry {
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
    Ok(())
}

/// Upsert a payment row atomically with its audit entry.
/// Uses `SET LOCAL lock_timeout` to prevent deadlocks with future flows.
/// Handles the insert race (concurrent INSERTs for the same external_id)
/// by catching the unique violation and retrying once as an UPDATE.
pub async fn upsert_payment(
    pool: &PgPool,
    payment: &NewPayment,
) -> Result<UpsertResult, PipelineError> {
    match try_upsert(pool, payment).await {
        Err(PipelineError::Database(ref e)) if is_unique_violation(e) => {
            // A concurrent transaction inserted the same external_id between
            // our SELECT and INSERT. Retry — the SELECT will now find the row.
            tracing::warn!(
                external_id = %payment.external_id(),
                "insert race detected, retrying as update"
            );
            try_upsert(pool, payment).await
        }
        other => other,
    }
}

fn is_unique_violation(err: &sqlx::Error) -> bool {
    err.as_database_error()
        .is_some_and(|e| e.is_unique_violation())
}

async fn try_upsert(
    pool: &PgPool,
    payment: &NewPayment,
) -> Result<UpsertResult, PipelineError> {
    let mut tx = pool.begin().await?;

    sqlx::query("SET LOCAL lock_timeout = '5s'")
        .execute(&mut *tx)
        .await?;

    let pg_amount: i64 = payment
        .money()
        .amount()
        .cents()
        .try_into()
        .map_err(|_| PipelineError::Validation("amount exceeds storage capacity".into()))?;

    let existing: Option<(Uuid, String)> = sqlx::query_as(
        "SELECT id, status FROM payments WHERE external_id = $1 FOR UPDATE",
    )
    .bind(payment.external_id())
    .fetch_optional(&mut *tx)
    .await?;

    match existing {
        None => {
            sqlx::query(
                r#"
                INSERT INTO payments (id, external_id, source, event_type, direction, amount, currency, status, metadata, raw_event, last_event_id, parent_external_id)
                VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
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
            .execute(&mut *tx)
            .await?;

            let audit = payment.audit_entry("webhook:stripe", "created");
            insert_audit_entry(&mut tx, &audit).await?;
            tx.commit().await?;
            Ok(UpsertResult::Created(payment.id()))
        }
        Some((id, old_status)) => {
            let old = PaymentStatus::try_from(old_status.as_str())?;

            if payment.status().rank() <= old.rank() {
                sqlx::query(
                    "UPDATE payments SET last_event_id = $1, updated_at = now() WHERE id = $2",
                )
                .bind(payment.last_event_id())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                let mut audit = payment.audit_entry("webhook:stripe", "event_received");
                audit.detail = serde_json::json!({
                    "event_type": payment.event_type(),
                    "current_status": old_status,
                    "incoming_status": payment.status().as_str(),
                    "skipped": true,
                });
                audit.entity_id = Some(id);
                insert_audit_entry(&mut tx, &audit).await?;
                tx.commit().await?;
                Ok(UpsertResult::Skipped(id))
            } else {
                sqlx::query(
                    r#"
                    UPDATE payments
                    SET status = $1, event_type = $2, metadata = $3, raw_event = $4, last_event_id = $5, updated_at = now()
                    WHERE id = $6
                    "#,
                )
                .bind(payment.status().as_str())
                .bind(payment.event_type())
                .bind(payment.metadata())
                .bind(payment.raw_event())
                .bind(payment.last_event_id())
                .bind(id)
                .execute(&mut *tx)
                .await?;

                let mut audit = payment.audit_entry("webhook:stripe", "status_changed");
                audit.detail = serde_json::json!({
                    "event_type": payment.event_type(),
                    "old_status": old_status,
                    "new_status": payment.status().as_str(),
                });
                audit.entity_id = Some(id);
                insert_audit_entry(&mut tx, &audit).await?;
                tx.commit().await?;
                Ok(UpsertResult::Updated(id))
            }
        }
    }
}
