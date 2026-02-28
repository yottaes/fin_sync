-- Add external_id for reliable linking (always available from Stripe)
ALTER TABLE audit_log ADD COLUMN external_id TEXT;

-- Make entity_id nullable — we may not have the internal row yet
-- Need to drop the rules temporarily to allow ALTER
DROP RULE no_update_audit ON audit_log;
ALTER TABLE audit_log ALTER COLUMN entity_id DROP NOT NULL;
CREATE RULE no_update_audit AS ON UPDATE TO audit_log DO INSTEAD NOTHING;

CREATE INDEX idx_audit_log_external_id ON audit_log(external_id);
