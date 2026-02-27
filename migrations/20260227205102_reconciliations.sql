-- 003_create_reconciliations.sql

CREATE TABLE reconciliations (
    id                  UUID PRIMARY KEY DEFAULT  uuidv7(), 
    payment_id          UUID NOT NULL REFERENCES payments(id),
    external_record_id  UUID REFERENCES external_records(id),
    status              TEXT NOT NULL,
    discrepancy_details JSONB,
    resolved_at         TIMESTAMPTZ,
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now()
);

-- discrepancy_details example:
-- {
--   "field": "amount",
--   "payment_value": 2000,
--   "external_value": 2500,
--   "difference": 500
-- }
