-- Add columns for proper event tracking
ALTER TABLE payments ADD COLUMN last_event_id TEXT;
ALTER TABLE payments ADD COLUMN updated_at TIMESTAMPTZ NOT NULL DEFAULT now();

-- Backfill last_event_id from current external_id (evt_xxx)
UPDATE payments SET last_event_id = external_id;

-- Drop unique constraint so we can backfill without conflicts
ALTER TABLE payments DROP CONSTRAINT payments_external_id_key;

-- Backfill: extract real object ID into external_id
UPDATE payments SET external_id = CASE
    WHEN raw_event->'data'->'object'->>'object' = 'payment_intent'
        THEN raw_event->'data'->'object'->>'id'
    WHEN raw_event->'data'->'object'->>'object' = 'refund'
        THEN raw_event->'data'->'object'->>'id'
    WHEN raw_event->'data'->'object'->>'object' = 'charge'
        THEN COALESCE(raw_event->'data'->'object'->>'payment_intent', raw_event->'data'->'object'->>'id')
    ELSE external_id
END;

-- Deduplicate: keep only the most recent row per external_id
DELETE FROM payments p
USING (
    SELECT external_id, MAX(received_at) AS max_received_at
    FROM payments
    GROUP BY external_id
    HAVING COUNT(*) > 1
) dupes
WHERE p.external_id = dupes.external_id
  AND p.received_at < dupes.max_received_at;

-- Re-add unique constraint
ALTER TABLE payments ADD CONSTRAINT payments_external_id_key UNIQUE (external_id);

ALTER TABLE payments ALTER COLUMN last_event_id SET NOT NULL;
CREATE INDEX idx_payments_last_event_id ON payments(last_event_id);
