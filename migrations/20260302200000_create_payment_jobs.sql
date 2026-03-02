CREATE TABLE payment_jobs (
    id           UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    event_id     TEXT NOT NULL UNIQUE,
    object_id    TEXT NOT NULL,
    event_type   TEXT NOT NULL,
    provider_ts  BIGINT NOT NULL,
    raw_event    JSONB NOT NULL,
    status       TEXT NOT NULL DEFAULT 'pending'
                 CHECK (status IN ('pending', 'processing', 'completed', 'failed')),
    attempts     INT NOT NULL DEFAULT 0,
    max_attempts INT NOT NULL DEFAULT 5,
    last_error   TEXT,
    scheduled_at TIMESTAMPTZ NOT NULL DEFAULT now(),
    created_at   TIMESTAMPTZ NOT NULL DEFAULT now(),
    updated_at   TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_payment_jobs_claimable
    ON payment_jobs (scheduled_at)
    WHERE status = 'pending';
