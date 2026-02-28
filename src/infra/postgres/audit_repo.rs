use {crate::domain::audit::NewAuditEntry, crate::domain::error::PipelineError};
pub async fn insert_audit_entry(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    entry: &NewAuditEntry,
) -> Result<(), PipelineError> {
    sqlx::query(
        r#"
        INSERT INTO audit_log (id, entity_type, entity_id, action, actor, detail)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(entry.id)
    .bind(&entry.entity_type)
    .bind(entry.entity_id)
    .bind(&entry.action)
    .bind(&entry.actor)
    .bind(&entry.detail)
    .execute(&mut **tx)
    .await?;

    Ok(())
}
