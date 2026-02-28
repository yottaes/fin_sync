CREATE TABLE payments (
    id                  UUID PRIMARY KEY DEFAULT uuidv7(),
    external_id         TEXT NOT NULL UNIQUE,
    source              TEXT NOT NULL,
    event_type          TEXT NOT NULL,
    direction           TEXT NOT NULL,
    amount              BIGINT NOT NULL,
    currency            TEXT NOT NULL,
    status              TEXT NOT NULL,
    metadata            JSONB NOT NULL DEFAULT '{}',
    raw_event           JSONB NOT NULL,
    last_event_id       TEXT NOT NULL,
    parent_external_id  TEXT,
    last_provider_ts    BIGINT NOT NULL,
    received_at         TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at          TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at          TIMESTAMPTZ NOT NULL DEFAULT now(),

    CONSTRAINT chk_payments_amount    CHECK (amount >= 0),
    CONSTRAINT chk_payments_status    CHECK (status IN ('pending', 'succeeded', 'failed', 'refunded')),
    CONSTRAINT chk_payments_direction CHECK (direction IN ('inbound', 'outbound')),
    CONSTRAINT chk_payments_currency  CHECK (currency IN ('usd', 'eur', 'gbp', 'jpy'))
);

CREATE INDEX idx_payments_status             ON payments(status);
CREATE INDEX idx_payments_direction          ON payments(direction);
CREATE INDEX idx_payments_last_event_id      ON payments(last_event_id);
CREATE INDEX idx_payments_parent_external_id ON payments(parent_external_id);
