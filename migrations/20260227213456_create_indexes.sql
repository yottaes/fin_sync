-- payments
CREATE INDEX idx_payments_status ON payments(status);
CREATE INDEX idx_payments_direction ON payments(direction);

-- external_records
CREATE INDEX idx_external_records_external_id ON external_records(external_id);
CREATE INDEX idx_external_records_direction ON external_records(direction);

-- reconciliations
CREATE INDEX idx_reconciliations_status ON reconciliations(status);
CREATE INDEX idx_reconciliations_payment_id ON reconciliations(payment_id);
CREATE INDEX idx_reconciliations_external_record_id ON reconciliations(external_record_id);

-- audit_log
CREATE INDEX idx_audit_log_entity ON audit_log(entity_type, entity_id);
CREATE INDEX idx_audit_log_created ON audit_log(created_at);
