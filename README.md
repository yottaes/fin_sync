# fin_sync

Payment synchronization service that sits between payment providers and ERP systems. Ingests payment events via webhooks, maintains canonical payment state, and keeps an immutable audit trail. Handles both directions: customer payments in (charges) and company payments out (refunds, vendor payouts).

## What it does today

- **Stripe webhook processing** — verifies signatures, normalizes PaymentIntent and Refund events into a unified payment model, logs charge events as passthrough.
- **State machine** — enforces valid status transitions (Pending -> Succeeded | Failed | Refunded). Rejects anomalous transitions, skips stale/duplicate events.
- **Concurrency control** — advisory locks serialize processing per payment object. No row-level lock contention, no insert races.
- **Dedup** — provider_events table catches duplicate webhook deliveries before any state mutation.
- **Audit log** — every state change, skip, and anomaly is recorded in the same transaction as the payment mutation. Append-only.
- **Compile-time SQL** — all production queries use `sqlx::query!` macros, verified against the real schema at build time. CI uses offline mode via `.sqlx/` metadata.

## Architecture

```
Payment Providers (Stripe, ...)
         |
         V
   [POST /webhook]
         |
    Signature verification
         |
    Event normalization (PI / Refund / Charge / unknown)
         |
    ┌────┴─────┐
    V          V
 Payments   Passthrough
 pipeline     audit
    |            |
    V            V
 provider_events (dedup)
    |
 advisory lock (serialize per object)
    |
 state machine check
    |
 ┌──┴──┐
 V     V
INSERT  UPDATE + audit_log
(new)   (transition)
```

## Data model

| Table | Purpose |
|-------|---------|
| `payments` | Canonical payment state. One row per PI or Refund (`external_id`). Tracks status, amount, currency, direction, last event. |
| `provider_events` | Dedup log. One row per Stripe event ID. |
| `audit_log` | Append-only. Records created/status_changed/event_received with JSONB detail. Keyed by `event_id` (unique). |
| `external_records` | ERP/external system records (schema ready, not yet populated). |
| `reconciliations` | Matching results between payments and external records (schema ready, not yet populated). |

## Key design decisions

- `external_id` = `pi_xxx` or `re_xxx` (the payment object), not `evt_xxx`. One row per payment, not per event.
- Status rank prevents regression: Pending(0) < Succeeded/Failed(1) < Refunded(2).
- Validation errors return 200 to Stripe (stop retry loop). DB errors return 500 (Stripe retries).
- Money is always `i64` cents + currency enum. No floats.

## Tech stack

Rust, Tokio, Axum, sqlx (Postgres, compile-time checked), async-stripe, tracing.

## Project structure

```
src/
  adapters/
    stripe.rs        # webhook handler, event dispatch, Stripe type conversion
    api_errors.rs    # PipelineError -> HTTP response mapping
  domain/
    payment.rs       # NewPayment, PaymentStatus, PaymentDirection, state machine
    money.rs         # MoneyAmount (u64 cents), Currency enum, Money
    audit.rs         # NewAuditEntry
    error.rs         # PipelineError
  infra/
    postgres/
      payment_repo.rs  # process_payment_event, log_passthrough_event (11 queries)
      audit_repo.rs    # insert_audit_entry (1 query)
  lib.rs             # AppState
  main.rs            # server setup, graceful shutdown
tests/
  payment_repo_test  # 20 integration tests (lifecycle, transitions, constraints)
  concurrency_test   # 4 tests (advisory locks, races, dedup under contention)
  passthrough_test   # 5 tests (charge/unknown event logging)
  property_test      # 5 property-based tests (money, status transitions)
migrations/          # 5 SQL migrations (payments, external_records, reconciliations, audit_log, provider_events)
.sqlx/               # compile-time query metadata (committed, used by CI offline mode)
```

## Running

```bash
# Requires Postgres running on localhost:5432
export DATABASE_URL="postgresql://postgres:password@localhost:5432/postgres"
export STRIPE_WEBHOOK_SECRET="whsec_..."

cargo run                # start server on :3000
stripe listen --forward-to localhost:3000/webhook  # forward Stripe events locally
cargo test               # run all 41 tests
```

## What's next

- **ERP data intake** — endpoints to receive structured records from ERP systems, populate `external_records`.
- **Reconciliation engine** — match payments against external records, write verdicts to `reconciliations`.
- **Status API** — query payment state and reconciliation status from outside (the "knock and check" interface).
- **Vendor payments** — outbound payments beyond refunds (AP, invoices), likely via additional provider adapters.
