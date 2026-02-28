CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT uuidv7(),
    entity_type TEXT NOT NULL,
    entity_id   UUID,
    external_id TEXT,
    event_id    TEXT,
    action      TEXT NOT NULL,
    actor       TEXT NOT NULL,
    detail      JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX idx_audit_log_event_id ON audit_log(event_id);
CREATE INDEX idx_audit_log_entity          ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_external_id     ON audit_log(external_id);
CREATE INDEX idx_audit_log_created         ON audit_log(created_at);
