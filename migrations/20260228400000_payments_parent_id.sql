-- Link refunds back to their parent payment (PI).
-- For inbound payments this is NULL; for refunds it's the PI's external_id.
ALTER TABLE payments ADD COLUMN parent_external_id TEXT;
CREATE INDEX idx_payments_parent_external_id ON payments(parent_external_id);
