# fin_sync

Payment synchronization service that sits between payment providers and ERP systems. Ingests payment events via webhooks, maintains canonical payment state, and keeps an immutable audit trail. Handles both directions: customer payments in (charges) and company payments out (refunds, vendor payouts).

## What it does today

- **Stripe webhook processing** — verifies signatures, normalizes PaymentIntent and Refund events into a unified payment model, logs charge events as passthrough.
- **Async job queue** — payment events are enqueued on webhook receipt (instant 200 response), processed in background by a Postgres-based worker. Passthrough events (charges, unknown) are still handled synchronously.
- **State machine** — enforces valid status transitions (Pending -> Succeeded | Failed | Refunded). Rejects anomalous transitions, skips stale/duplicate events.
- **Concurrency control** — advisory locks serialize processing per payment object. No row-level lock contention, no insert races.
- **Dedup** — `payment_jobs` dedup by `event_id` at enqueue time; `provider_events` catches duplicates again before state mutation.
- **Retry & recovery** — failed jobs retry with exponential backoff (2^attempts sec, max 5 attempts). A reaper resets stuck `processing` jobs after 2 minutes.
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
    ┌────────┴─────────┐
    V                  V
 Payment events     Passthrough
 enqueue to           audit
 payment_jobs        (sync)
    |
    V
 ┌─────────────────────────────┐
 │  Background worker (1s poll) │
 │  claim → fetch API → process │
 └─────────────────────────────┘
    |
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
    |
 job: complete / fail (retry)

 ┌───────────────────────────┐
 │  Stale job reaper (60s)   │
 │  processing > 2min → reset │
 └───────────────────────────┘
```

## Data model

| Table | Purpose |
|-------|---------|
| `payments` | Canonical payment state. One row per PI or Refund (`external_id`). Tracks status, amount, currency, direction, last event. |
| `payment_jobs` | Async job queue. One row per webhook event. Tracks status (pending/processing/completed/failed), attempts, backoff. |
| `provider_events` | Dedup log. One row per Stripe event ID. |
| `audit_log` | Append-only. Records created/status_changed/event_received with JSONB detail. Keyed by `event_id` (unique). |
| `external_records` | ERP/external system records (schema ready, not yet populated). |
| `reconciliations` | Matching results between payments and external records (schema ready, not yet populated). |

## Key design decisions

- `external_id` = `pi_xxx` or `re_xxx` (the payment object), not `evt_xxx`. One row per payment, not per event.
- Status rank prevents regression: Pending(0) < Succeeded/Failed(1) < Refunded(2).
- Webhook returns 200 immediately after enqueue — prevents Stripe retry storms if provider API is slow.
- Validation errors return 200 to Stripe (stop retry loop). DB errors return 500 (Stripe retries).
- Money is always `i64` cents + currency enum. No floats.

## Tech stack

Rust, Tokio, Axum, sqlx (Postgres, compile-time checked), async-stripe, tracing.

## Project structure

```
src/
  adapters/
    stripe/
      webhook.rs     # signature verification, event dispatch, enqueue
      client.rs      # StripeProvider (API fetches)
  transport/
    http/errors.rs   # PipelineError -> HTTP response mapping
  domain/
    payment.rs       # NewPayment, PaymentStatus, PaymentDirection, state machine
    money.rs         # MoneyAmount (i64 cents), Currency enum, Money
    audit.rs         # NewAuditEntry
    error.rs         # PipelineError
    provider.rs      # PaymentProvider trait
    id.rs            # ExternalId, EventId newtypes
  services/
    payment_pipeline.rs  # fetch_and_process_payment, process_payment_event, handle_passthrough
    worker.rs            # run_worker (1s poll), run_reaper (60s stale reset)
  infra/
    postgres/
      payment_repo.rs  # insert/update/dedup queries
      audit_repo.rs    # insert_audit_entry
      job_repo.rs      # enqueue, claim, complete, fail, reap_stale
  lib.rs             # AppState
  main.rs            # server setup, worker spawn, graceful shutdown
tests/
  payment_repo_test  # 20 integration tests (lifecycle, transitions, constraints)
  concurrency_test   # 4 tests (advisory locks, races, dedup under contention)
  passthrough_test   # 5 tests (charge/unknown event logging)
  property_test      # 5 property-based tests (money, status transitions)
migrations/          # 7 SQL migrations
.sqlx/               # compile-time query metadata (committed, used by CI offline mode)
```

## Running

```bash
# Copy and fill in your Stripe keys
cp .env.example .env

# Requires Postgres running on localhost:5432
# Requires Stripe CLI for local webhook forwarding
stripe listen --forward-to localhost:3000/webhook  # note the whsec_ secret it prints

# Edit .env with your values:
#   DATABASE_URL=postgresql://postgres:password@localhost:5432/postgres
#   STRIPE_WEBHOOK_SECRET=whsec_...   (from stripe listen output)
#   STRIPE_SECRET_KEY=sk_test_...     (from Stripe dashboard)

cargo run                # start server on :3000
cargo test               # run all 41 tests
```

## What's next

- **ERP data intake** — endpoints to receive structured records from ERP systems, populate `external_records`.
- **Reconciliation engine** — match payments against external records, write verdicts to `reconciliations`.
- **Status API** — query payment state and reconciliation status from outside (the "knock and check" interface).
- **Vendor payments** — outbound payments beyond refunds (AP, invoices), likely via additional provider adapters.
