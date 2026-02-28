-- Defense-in-depth: enforce valid values at the DB level.
DO $$ BEGIN
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_payments_amount') THEN
        ALTER TABLE payments ADD CONSTRAINT chk_payments_amount CHECK (amount >= 0);
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_payments_status') THEN
        ALTER TABLE payments ADD CONSTRAINT chk_payments_status CHECK (status IN ('pending', 'succeeded', 'failed', 'refunded'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_payments_direction') THEN
        ALTER TABLE payments ADD CONSTRAINT chk_payments_direction CHECK (direction IN ('inbound', 'outbound'));
    END IF;
    IF NOT EXISTS (SELECT 1 FROM pg_constraint WHERE conname = 'chk_payments_currency') THEN
        ALTER TABLE payments ADD CONSTRAINT chk_payments_currency CHECK (currency IN ('usd', 'eur', 'gbp', 'jpy'));
    END IF;
END $$;
