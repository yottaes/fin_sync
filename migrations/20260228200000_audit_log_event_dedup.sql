-- Add event_id for deduplication of Stripe retries
ALTER TABLE audit_log ADD COLUMN event_id TEXT;
CREATE UNIQUE INDEX idx_audit_log_event_id ON audit_log(event_id);
