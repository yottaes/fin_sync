use crate::domain::error::PipelineError;

pub struct JobRow {
    pub id: uuid::Uuid,
    pub event_id: String,
    pub object_id: String,
    pub event_type: String,
    pub provider_ts: i64,
    pub raw_event: serde_json::Value,
    pub attempts: i32,
}

/// Enqueue a webhook event for async processing.
/// Returns `true` if inserted, `false` if duplicate (already enqueued).
pub async fn enqueue(
    pool: &sqlx::PgPool,
    event_id: &str,
    object_id: &str,
    event_type: &str,
    provider_ts: i64,
    raw_event: &serde_json::Value,
) -> Result<bool, PipelineError> {
    let inserted: Option<bool> = sqlx::query_scalar!(
        r#"
        INSERT INTO payment_jobs (event_id, object_id, event_type, provider_ts, raw_event)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (event_id) DO NOTHING
        RETURNING true AS "inserted!"
        "#,
        event_id,
        object_id,
        event_type,
        provider_ts,
        raw_event,
    )
    .fetch_optional(pool)
    .await?;

    Ok(inserted.is_some())
}

/// Claim up to `limit` pending jobs for processing.
/// Uses SKIP LOCKED to avoid contention with other workers.
pub async fn claim(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    limit: i64,
) -> Result<Vec<JobRow>, PipelineError> {
    let rows = sqlx::query_as!(
        JobRow,
        r#"
        UPDATE payment_jobs
        SET status = 'processing', updated_at = now()
        WHERE id IN (
            SELECT id FROM payment_jobs
            WHERE status = 'pending' AND scheduled_at <= now()
            ORDER BY scheduled_at
            LIMIT $1
            FOR UPDATE SKIP LOCKED
        )
        RETURNING id, event_id, object_id, event_type, provider_ts, raw_event, attempts
        "#,
        limit,
    )
    .fetch_all(&mut **tx)
    .await?;

    Ok(rows)
}

/// Mark a job as completed.
pub async fn complete(pool: &sqlx::PgPool, id: uuid::Uuid) -> Result<(), PipelineError> {
    sqlx::query!(
        "UPDATE payment_jobs SET status = 'completed', updated_at = now() WHERE id = $1",
        id,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Record a failure. Exponential backoff via scheduled_at.
/// If max attempts reached, mark as 'failed' permanently.
pub async fn fail(pool: &sqlx::PgPool, id: uuid::Uuid, error: &str) -> Result<(), PipelineError> {
    sqlx::query!(
        r#"
        UPDATE payment_jobs
        SET attempts = attempts + 1,
            last_error = $2,
            status = CASE
                WHEN attempts + 1 >= max_attempts THEN 'failed'
                ELSE 'pending'
            END,
            scheduled_at = CASE
                WHEN attempts + 1 >= max_attempts THEN scheduled_at
                ELSE now() + make_interval(secs => power(2, attempts + 1)::int)
            END,
            updated_at = now()
        WHERE id = $1
        "#,
        id,
        error,
    )
    .execute(pool)
    .await?;
    Ok(())
}

/// Reset jobs stuck in 'processing' for >2 minutes back to 'pending'.
/// Returns the number of reaped jobs.
pub async fn reap_stale(pool: &sqlx::PgPool) -> Result<u64, PipelineError> {
    let result = sqlx::query!(
        r#"
        UPDATE payment_jobs
        SET status = 'pending', updated_at = now()
        WHERE status = 'processing' AND updated_at < now() - interval '2 minutes'
        "#,
    )
    .execute(pool)
    .await?;

    Ok(result.rows_affected())
}
