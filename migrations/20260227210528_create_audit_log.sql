CREATE TABLE audit_log (
    id          UUID PRIMARY KEY DEFAULT uuidv7(),
    entity_type TEXT NOT NULL,
    entity_id   UUID NOT NULL,
    action      TEXT NOT NULL,
    actor       TEXT NOT NULL,
    detail      JSONB NOT NULL DEFAULT '{}',
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE RULE no_update_audit AS ON UPDATE TO audit_log DO INSTEAD NOTHING;
CREATE RULE no_delete_audit AS ON DELETE TO audit_log DO INSTEAD NOTHING;
