CREATE TABLE reconciliations (
    id                  UUID PRIMARY KEY DEFAULT uuidv7(),
    payment_id          UUID NOT NULL REFERENCES payments(id),
    external_record_id  UUID REFERENCES external_records(id),
    status              TEXT NOT NULL,
    discrepancy_details JSONB,
    resolved_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_reconciliations_status             ON reconciliations(status);
CREATE INDEX idx_reconciliations_payment_id         ON reconciliations(payment_id);
CREATE INDEX idx_reconciliations_external_record_id ON reconciliations(external_record_id);
