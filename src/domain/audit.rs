use uuid::Uuid;

pub struct NewAuditEntry {
    pub id: Uuid,
    pub entity_type: String,
    pub entity_id: Option<Uuid>,
    pub external_id: Option<String>,
    pub event_id: String,
    pub action: String,
    pub actor: String,
    pub detail: serde_json::Value,
}
