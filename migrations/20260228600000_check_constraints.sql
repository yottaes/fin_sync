-- Defense-in-depth: enforce valid values at the DB level.
ALTER TABLE payments ADD CONSTRAINT chk_payments_amount
    CHECK (amount >= 0);

ALTER TABLE payments ADD CONSTRAINT chk_payments_status
    CHECK (status IN ('pending', 'succeeded', 'failed', 'refunded'));

ALTER TABLE payments ADD CONSTRAINT chk_payments_direction
    CHECK (direction IN ('inbound', 'outbound'));

ALTER TABLE payments ADD CONSTRAINT chk_payments_currency
    CHECK (currency IN ('usd', 'eur', 'gbp', 'jpy'));
