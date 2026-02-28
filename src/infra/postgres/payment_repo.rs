use {
    super::audit_repo::insert_audit_entry, crate::domain::audit::NewAuditEntry,
    crate::domain::error::PipelineError, crate::domain::payment::NewPayment, sqlx::PgPool,
    uuid::Uuid,
};

#[derive(Debug)]
pub enum InsertResult {
    Created(Uuid),
    Duplicate,
}

pub async fn insert_payment(
    pool: &PgPool,
    payment: &NewPayment,
    audit: &NewAuditEntry,
) -> Result<InsertResult, PipelineError> {
    let mut tx = pool.begin().await?;

    let pg_amount: i64 = payment
        .money()
        .amount()
        .cents()
        .try_into()
        .map_err(|_| PipelineError::Validation("amount exceeds storage capacity".into()))?;

    let row: Option<(Uuid,)> = sqlx::query_as(
        r#"
        INSERT INTO payments (id, external_id, source, event_type, direction, amount, currency, status, metadata, raw_event)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10)
        ON CONFLICT (external_id) DO NOTHING
        RETURNING id
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
    .fetch_optional(&mut *tx)
    .await?;

    match row {
        Some((id,)) => {
            insert_audit_entry(&mut tx, audit).await?;
            tx.commit().await?;
            Ok(InsertResult::Created(id))
        }
        None => {
            tx.rollback().await?;
            Ok(InsertResult::Duplicate)
        }
    }
}
