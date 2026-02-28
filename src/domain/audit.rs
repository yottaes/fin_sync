use uuid::Uuid;

pub struct NewAuditEntry {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Uuid,
    pub action: String,
    pub actor: String,
    pub detail: serde_json::Value,
}
