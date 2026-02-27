# fin-sync (Financial Reconciliation Gateway)

A high-reliability payment reconciliation and synchronization service built in Rust. `fin-sync` acts as a **judge layer** between payment providers, ERP systems, and CRMs. It verifies financial data consistency across all systems, synchronizes statuses, and maintains an immutable audit trail.

Generic by design: it plugs into any financial pipeline where multiple systems must agree on payment state. It is not a payment processor, nor a parser — it is a pure verification and synchronization engine.

## 🚀 Core Philosophy

- **The Judge, Not the Extractor:** The service expects upstream systems to provide structured, typed data (JSON). It compares states and issues verdicts; it does not parse raw documents.
- **Strict Schema Validation (Fail Fast):** Incoming data must strictly conform to expected schemas. Missing fields, wrong types, or out-of-bounds values immediately return a `422 Unprocessable Entity`. No partial parsing, no silent failures.
- **Type-Driven Money Handling:** All monetary amounts are represented as `i64` in the smallest currency unit (e.g., cents). Floating-point numbers are strictly prohibited. The custom `MoneyAmount` type is always paired with a `Currency` enum, making currency mismatches a compile-time error.
- **Pure Push Model (MVP):** `fin-sync` relies entirely on webhooks and external data pushes. It does not poll or fetch data from external APIs, ensuring predictable load and immediate reactivity.

## 🏗 Architecture & Data Flow

External systems push data into the service. Validation and persistence happen immediately. Background tasks process the reconciliation asynchronously, ensuring the HTTP layer remains fast and decoupled from heavy logic.

    Payment Providers (Stripe, etc.)
             |
             V
       [Webhook Endpoints] ──→ Event Validation ──→ Postgres (payments table)
                                                            |
    External Systems (ERP, CRM)                             V
             |                                      tokio::spawn(pipeline)
             V                                              |
     [Data Intake Endpoints] ──→ Schema Validation ──→ Postgres
                                                   (external_records)
                                                            |
                                                            V
                                                  Reconciliation Engine
                                                         (Judge)
                                                            |
                                                  ┌─────────┼─────────┐
                                                  V         V         V
                                            Postgres   Postgres   Postgres
                                    (reconciliations)(audit_log)(analytics)
                                                  |
                                                  V
                                              [REST API]
                                      Status / Reports / Analytics

## 🔌 Adapter Pattern (Validation)

External data sources are abstracted behind validation traits. Adding a new system simply means implementing the corresponding trait:

- `PaymentProvider` — Validate and normalize incoming payment webhooks (e.g., Stripe signature verification).
- `ERPValidator` — Validate incoming ERP records.
- `CRMValidator` — Validate incoming CRM deal statuses.

## 🗄️ Data Model

- **`payments`**: Core payment records (`id`, `external_id`, `amount` as `i64`, `currency`, `idempotency_key`).
- **`external_records`**: Structured data from ERP/CRM (`source`, `external_id`, `record_type`, `amount`, `currency`, `raw_data`).
- **`reconciliations`**: Matching results (`status`: matched/mismatch/pending/manual_review, `discrepancy_details`).
- **`audit_log`**: Append-only event log (`entity_type`, `action`, `actor`, `detail`). No `UPDATE` or `DELETE` permitted.
- **Analytics**: Aggregated views over reconciliation data (match rates, discrepancies).

## 🛡️ Reliability Guarantees

- **Idempotency:** Duplicate submissions are detected and ignored via deterministic provider IDs or `X-Idempotency-Key` headers. Critical for provider retries.
- **Transactional Audit:** Any state change (e.g., a new reconciliation verdict) is committed to the database in the same SQL transaction as its corresponding `audit_log` entry.
- **Error Handling:** All pipeline stages return typed `Result<T, PipelineError>`. Failed stages are logged with context and marked for retry or manual review. `unwrap()` is forbidden in production paths.
- **Backpressure:** Internal bounded `tokio::mpsc` channels ensure that if the database writer falls behind, the async producers wait rather than dropping data.

## 🛠️ Tech Stack

- **Runtime:** Rust, Tokio
- **Web Framework:** Axum, Tower (rate limiting, timeout, request tracing)
- **Database:** PostgreSQL via `sqlx` (compile-time checked queries, transactions)
- **Logging:** `tracing` + `tracing-subscriber` (structured, span-based)
- **Serialization:** `serde`, `serde_json` (strict deserialization)
- **Infrastructure:** Docker Compose

## 📂 Project Structure

    fin-sync/
    ├── Cargo.toml
    ├── Dockerfile
    ├── docker-compose.yml
    ├── proto/                     # Future gRPC definitions
    ├── migrations/
    │   ├── 001_create_payments.sql
    │   ├── 002_create_external_records.sql
    │   ├── 003_create_reconciliations.sql
    │   └── 004_create_audit_log.sql
    ├── src/
    │   ├── main.rs
    │   ├── config.rs
    │   ├── routes/                # Axum HTTP handlers
    │   ├── domain/                # Core types (MoneyAmount, Currency, errors)
    │   ├── adapters/              # Validation traits (Stripe, ERP, CRM)
    │   ├── pipeline/              # Async processing & matching logic
    │   ├── services/              # Business logic (Reconciler, Audit, Analytics)
    │   └── db/                    # sqlx queries and connection pool
    └── README.md

## 🗺️ Future Extensions

- **Internal gRPC API:** High-performance inter-service communication (`tonic` + `prost`).
- **Pull Model (Cron):** Scheduled fetching from legacy external APIs that do not support webhooks.
- **Analytical Engine:** DuckDB read-only layer over Postgres data for advanced reporting.
- **Real-time Alerting:** Webhook callbacks, Slack/email notifications on mismatches.
- **Multi-Currency:** Exchange rate snapshots for cross-currency reconciliation.
