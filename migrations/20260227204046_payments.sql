CREATE TABLE payments (
    id          UUID PRIMARY KEY DEFAULT uuidv7(),
    external_id TEXT NOT NULL UNIQUE,
    source      TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    direction   TEXT NOT NULL,  
    amount      BIGINT NOT NULL,
    currency    TEXT NOT NULL,
    status      TEXT NOT NULL,
    metadata    JSONB NOT NULL DEFAULT '{}',
    raw_event   JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at  TIMESTAMPTZ NOT NULL DEFAULT now()
);
