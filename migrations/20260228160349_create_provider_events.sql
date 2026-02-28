CREATE TABLE provider_events (
    event_id    TEXT PRIMARY KEY,
    object_id   TEXT NOT NULL,
    event_type  TEXT NOT NULL,
    provider_ts BIGINT NOT NULL,
    payload     JSONB NOT NULL,
    received_at TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_provider_events_object_id ON provider_events(object_id);
