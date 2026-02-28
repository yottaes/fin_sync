-- Rename Stripe-specific names to provider-agnostic ones (idempotent).
DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM pg_tables WHERE tablename = 'stripe_events') THEN
        ALTER TABLE stripe_events RENAME TO provider_events;
    END IF;
END $$;

DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM pg_indexes WHERE indexname = 'idx_stripe_events_object_id') THEN
        ALTER INDEX idx_stripe_events_object_id RENAME TO idx_provider_events_object_id;
    END IF;
END $$;

DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'provider_events' AND column_name = 'stripe_created') THEN
        ALTER TABLE provider_events RENAME COLUMN stripe_created TO provider_ts;
    END IF;
END $$;

DO $$ BEGIN
    IF EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'payments' AND column_name = 'last_stripe_created') THEN
        ALTER TABLE payments RENAME COLUMN last_stripe_created TO last_provider_ts;
    END IF;
END $$;
