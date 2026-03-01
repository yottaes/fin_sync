use {
    crate::domain::error::PipelineError,
    crate::domain::payment::{ExistingPayment, NewPayment, PaymentStatus},
    uuid::Uuid,
};

/// Record a Stripe event for dedup. Returns `true` if newly inserted, `false` if duplicate.
pub async fn insert_provider_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    event_id: &str,
    object_id: &str,
    event_type: &str,
    provider_ts: i64,
    payload: &serde_json::Value,
) -> Result<bool, PipelineError> {
    let inserted: Option<bool> = sqlx::query_scalar!(
        r#"
        INSERT INTO provider_events (event_id, object_id, event_type, provider_ts, payload)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (event_id) DO NOTHING
        RETURNING true AS "inserted!"
        "#,
        event_id,
        object_id,
        event_type,
        provider_ts,
        payload,
    )
    .fetch_optional(&mut **tx)
    .await?;

    Ok(inserted.is_some())
}

/// Fetch the current state of a payment by external_id.
pub async fn get_existing_payment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    external_id: &str,
) -> Result<Option<ExistingPayment>, PipelineError> {
    let row = sqlx::query!(
        "SELECT id, status, last_provider_ts FROM payments WHERE external_id = $1",
        external_id,
    )
    .fetch_optional(&mut **tx)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => {
            let status = PaymentStatus::try_from(r.status.as_str())?;
            Ok(Some(ExistingPayment {
                id: r.id,
                status,
                last_provider_ts: r.last_provider_ts,
            }))
        }
    }
}

/// Look up a payment's UUID by external_id (for linking audit entries).
pub async fn find_payment_id(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    external_id: &str,
) -> Result<Option<Uuid>, PipelineError> {
    let id = sqlx::query_scalar!(
        "SELECT id FROM payments WHERE external_id = $1",
        external_id
    )
    .fetch_optional(&mut **tx)
    .await?;
    Ok(id)
}

/// Insert a brand-new payment row.
pub async fn insert_payment(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    payment: &NewPayment,
) -> Result<(), PipelineError> {
    let pg_amount: i64 = payment.money().amount().cents();
    sqlx::query!(
        r#"
        INSERT INTO payments
            (id, external_id, source, event_type, direction,
             amount, currency, status, metadata, raw_event,
             last_event_id, parent_external_id, last_provider_ts)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13)
        "#,
        payment.id(),
        payment.external_id(),
        payment.source(),
        payment.event_type(),
        payment.direction().as_str(),
        pg_amount,
        payment.money().currency().as_str(),
        payment.status().as_str(),
        payment.metadata(),
        payment.raw_event(),
        payment.last_event_id(),
        payment.parent_external_id(),
        payment.provider_ts(),
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Advance payment status + tracking fields (for valid transitions).
pub async fn update_payment_status(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    payment: &NewPayment,
) -> Result<(), PipelineError> {
    sqlx::query!(
        r#"
        UPDATE payments
        SET status = $1, event_type = $2, metadata = $3,
            last_event_id = $4, last_provider_ts = $5, updated_at = now()
        WHERE id = $6
        "#,
        payment.status().as_str(),
        payment.event_type(),
        payment.metadata(),
        payment.last_event_id(),
        payment.provider_ts(),
        id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Update only event tracking (temporal stale — incoming ts is older, don't touch last_provider_ts).
pub async fn touch_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    event_id: &str,
) -> Result<(), PipelineError> {
    sqlx::query!(
        "UPDATE payments SET last_event_id = $1, updated_at = now() WHERE id = $2",
        event_id,
        id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}

/// Update event tracking + advance timestamp (same-status, anomaly).
pub async fn touch_event_with_ts(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    id: Uuid,
    event_id: &str,
    provider_ts: i64,
) -> Result<(), PipelineError> {
    sqlx::query!(
        "UPDATE payments SET last_event_id = $1, last_provider_ts = GREATEST(last_provider_ts, $2), updated_at = now() WHERE id = $3",
        event_id,
        provider_ts,
        id,
    )
    .execute(&mut **tx)
    .await?;
    Ok(())
}
