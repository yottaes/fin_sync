use {
    crate::domain::{
        error::PipelineError,
        id::ExternalId,
        money::Currency,
        payment::{
            ExistingPayment, NewPayment, PaymentDirection, PaymentFilters, PaymentStatus,
            PaymentView,
        },
    },
    sqlx::PgPool,
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
        "SELECT id, status FROM payments WHERE external_id = $1",
        external_id,
    )
    .fetch_optional(&mut **tx)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => {
            let status = PaymentStatus::try_from(r.status.as_str())?;
            Ok(Some(ExistingPayment { id: r.id, status }))
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
// NOTE: raw_event is intentionally NOT updated here.
// It preserves the creation snapshot; latest event payload
// is always available in provider_events by last_event_id.
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

pub async fn get_payment_by_id(
    pool: &PgPool,
    id: ExternalId,
) -> Result<Option<PaymentView>, PipelineError> {
    let row = sqlx::query!(
        r#"SELECT 
            external_id, 
            source, 
            status, 
            amount, 
            currency, 
            direction, 
            updated_at, 
            created_at
           FROM payments
           WHERE external_id = $1 
        "#,
        id.as_str()
    )
    .fetch_optional(pool)
    .await?;

    match row {
        None => Ok(None),
        Some(r) => Ok(Some(PaymentView {
            id: ExternalId::new(r.external_id)?,
            source: r.source,
            status: PaymentStatus::try_from(r.status.as_str())?,
            amount: r.amount,
            currency: Currency::try_from(r.currency.as_str())?,
            direction: PaymentDirection::try_from(r.direction.as_str())?,
            created_at: r.created_at,
            updated_at: r.updated_at,
        })),
    }
}

pub async fn get_list_payments(
    pool: &PgPool,
    filters: PaymentFilters,
) -> Result<Vec<PaymentView>, PipelineError> {
    let status = filters.status.map(|s| s.as_str().to_owned());
    let currency = filters.currency.map(|c| c.as_str().to_owned());
    let direction = filters.direction.map(|d| d.as_str().to_owned());
    let limit = filters.limit.expect("limit must be set by service layer") as i64;
    let rows = sqlx::query!(
        r#"
            SELECT
                external_id,
                source,
                status,
                amount,
                currency,
                direction,
                updated_at,
                created_at
            FROM payments
            WHERE ($1::text IS NULL OR source = $1)
                AND ($2::text IS NULL OR status = $2)
                AND ($3::bigint IS NULL OR amount >= $3)
                AND ($4::bigint IS NULL OR amount <= $4)
                AND ($5::text IS NULL OR currency = $5)
                AND ($6::text IS NULL OR direction = $6)
                AND ($7::timestamptz IS NULL OR created_at >= $7)
                AND ($8::timestamptz IS NULL OR created_at <= $8)
            ORDER BY created_at DESC
            LIMIT $9 OFFSET $10
        "#,
        filters.source,
        status as Option<String>,
        filters.amount_min,
        filters.amount_max,
        currency as Option<String>,
        direction as Option<String>,
        filters.start_date,
        filters.end_date,
        limit,
        filters.offset,
    )
    .fetch_all(pool)
    .await?;

    rows.into_iter()
        .map(|r| {
            Ok(PaymentView {
                id: ExternalId::new(r.external_id)?,
                source: r.source,
                status: PaymentStatus::try_from(r.status.as_str())?,
                amount: r.amount,
                currency: Currency::try_from(r.currency.as_str())?,
                direction: PaymentDirection::try_from(r.direction.as_str())?,
                created_at: r.created_at,
                updated_at: r.updated_at,
            })
        })
        .collect()
}
