use {crate::domain::audit::NewAuditEntry, crate::domain::error::PipelineError};

pub async fn insert_audit_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entry: &NewAuditEntry,
) -> Result<bool, PipelineError> {
    let result = sqlx::query!(
        r#"
        INSERT INTO audit_log (id, entity_type, entity_id, external_id, event_id, action, actor, detail)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8)
        ON CONFLICT (event_id) DO NOTHING
        "#,
        entry.id,
        &entry.entity_type,
        entry.entity_id,
        entry.external_id.as_deref(),
        &entry.event_id,
        &entry.action,
        &entry.actor,
        &entry.detail,
    )
    .execute(&mut **tx)
    .await?;

    Ok(result.rows_affected() > 0)
}
