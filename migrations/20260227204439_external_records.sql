CREATE TABLE external_records (
    id              UUID PRIMARY KEY DEFAULT uuidv7(),
    source          TEXT NOT NULL,
    external_id     TEXT NOT NULL,
    idempotency_key TEXT NOT NULL UNIQUE,
    record_type     TEXT NOT NULL,
    direction       TEXT NOT NULL,  -- 'inbound' / 'outbound'
    amount          BIGINT NOT NULL,
    currency        TEXT NOT NULL,
    status          TEXT NOT NULL,
    raw_data        JSONB NOT NULL DEFAULT '{}',
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);
