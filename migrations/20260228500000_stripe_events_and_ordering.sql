-- Append-only log of every Stripe event received.
-- Source of truth for dedup and raw payload history.
CREATE TABLE stripe_events (
    event_id        TEXT PRIMARY KEY,
    object_id       TEXT NOT NULL,
    event_type      TEXT NOT NULL,
    stripe_created  BIGINT NOT NULL,
    payload         JSONB NOT NULL,
    received_at     TIMESTAMPTZ NOT NULL DEFAULT now()
);

CREATE INDEX idx_stripe_events_object_id ON stripe_events(object_id);

-- Add Stripe's event timestamp to payments for temporal ordering.
-- Replaces the rank-based ordering system.
ALTER TABLE payments ADD COLUMN last_stripe_created BIGINT;

-- Backfill from raw_event: Stripe events have a top-level "created" field (unix ts).
UPDATE payments SET last_stripe_created = (raw_event->>'created')::bigint
WHERE raw_event->>'created' IS NOT NULL;

-- For any rows that didn't have it, use extract(epoch) from received_at as fallback.
UPDATE payments SET last_stripe_created = extract(epoch from received_at)::bigint
WHERE last_stripe_created IS NULL;

ALTER TABLE payments ALTER COLUMN last_stripe_created SET NOT NULL;
