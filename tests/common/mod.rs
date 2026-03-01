#![allow(dead_code)]

use fin_sync::domain::id::{EventId, ExternalId};
use fin_sync::domain::money::{Currency, Money, MoneyAmount};
use fin_sync::domain::payment::{NewPayment, NewPaymentParams, PaymentDirection, PaymentStatus};
use sqlx::PgPool;
use std::sync::Once;

const ADMIN_DB_URL: &str = "postgresql://postgres:password@localhost:5432/postgres";

static INIT_ONCE: Once = Once::new();

/// Creates a dedicated database for this test binary, runs migrations, and truncates.
/// Each binary gets full isolation — no cross-binary interference.
///
/// `db_name` should be unique per test file (e.g. "fin_sync_test_payment", "fin_sync_test_concurrency").
pub async fn setup_pool(db_name: &str) -> PgPool {
    let db_url = format!("postgresql://postgres:password@localhost:5432/{db_name}");

    // Create DB + migrate + truncate once per binary.
    // Runs on a separate thread to avoid nested-runtime panic.
    let db_name_owned = db_name.to_string();
    let db_url_owned = db_url.clone();
    INIT_ONCE.call_once(move || {
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .expect("failed to build init runtime");
            rt.block_on(async {
                // Connect to admin DB to create the test database.
                let admin = PgPool::connect(ADMIN_DB_URL)
                    .await
                    .expect("failed to connect to admin db");
                // CREATE DATABASE is not idempotent, so check first.
                let exists: bool = sqlx::query_scalar(
                    "SELECT EXISTS(SELECT 1 FROM pg_database WHERE datname = $1)",
                )
                .bind(&db_name_owned)
                .fetch_one(&admin)
                .await
                .expect("failed to check db existence");
                if !exists {
                    sqlx::query(&format!("CREATE DATABASE {db_name_owned}"))
                        .execute(&admin)
                        .await
                        .expect("failed to create test db");
                }
                admin.close().await;

                // Migrate + truncate the test database.
                let pool = PgPool::connect(&db_url_owned)
                    .await
                    .expect("failed to connect to test db");
                sqlx::migrate!("./migrations")
                    .run(&pool)
                    .await
                    .expect("failed to run migrations");
                sqlx::query("TRUNCATE payments, audit_log, provider_events, reconciliations, external_records RESTART IDENTITY CASCADE")
                    .execute(&pool)
                    .await
                    .expect("truncate failed");
                pool.close().await;
            });
        })
        .join()
        .expect("init thread panicked");
    });

    let pool = PgPool::connect(&db_url)
        .await
        .expect("failed to connect to test db");

    sqlx::migrate!("./migrations")
        .run(&pool)
        .await
        .expect("failed to run migrations");

    pool
}

/// Build an inbound (PaymentIntent) payment with sensible defaults.
pub fn make_payment(
    external_id: &str,
    event_id: &str,
    status: PaymentStatus,
    provider_ts: i64,
) -> NewPayment {
    NewPayment::new(NewPaymentParams {
        external_id: ExternalId::new(external_id).unwrap(),
        source: "stripe".to_string(),
        event_type: format!("payment_intent.{}", status.as_str()),
        direction: PaymentDirection::Inbound,
        money: Money::new(MoneyAmount::new(5000).unwrap(), Currency::Usd),
        status,
        metadata: serde_json::json!({}),
        raw_event: serde_json::json!({"id": event_id}),
        last_event_id: EventId::new(event_id).unwrap(),
        parent_external_id: None,
        provider_ts,
    })
}

/// Build an outbound (Refund) payment.
pub fn make_refund(
    external_id: &str,
    event_id: &str,
    status: PaymentStatus,
    provider_ts: i64,
    parent_external_id: &str,
) -> NewPayment {
    NewPayment::new(NewPaymentParams {
        external_id: ExternalId::new(external_id).unwrap(),
        source: "stripe".to_string(),
        event_type: format!("charge.refund.{}", status.as_str()),
        direction: PaymentDirection::Outbound,
        money: Money::new(MoneyAmount::new(5000).unwrap(), Currency::Usd),
        status,
        metadata: serde_json::json!({}),
        raw_event: serde_json::json!({"id": event_id}),
        last_event_id: EventId::new(event_id).unwrap(),
        parent_external_id: Some(ExternalId::new(parent_external_id).unwrap()),
        provider_ts,
    })
}

// ── Query helpers ──────────────────────────────────────────────────────────

pub struct PaymentRow {
    pub id: uuid::Uuid,
    pub external_id: String,
    pub status: String,
    pub last_event_id: String,
    pub parent_external_id: Option<String>,
    pub last_provider_ts: i64,
    pub direction: String,
    pub amount: i64,
    pub currency: String,
}

pub async fn get_payment(pool: &PgPool, external_id: &str) -> Option<PaymentRow> {
    sqlx::query_as::<_, (uuid::Uuid, String, String, String, Option<String>, i64, String, i64, String)>(
        "SELECT id, external_id, status, last_event_id, parent_external_id, last_provider_ts, direction, amount, currency FROM payments WHERE external_id = $1",
    )
    .bind(external_id)
    .fetch_optional(pool)
    .await
    .expect("query failed")
    .map(|(id, external_id, status, last_event_id, parent_external_id, last_provider_ts, direction, amount, currency)| {
        PaymentRow { id, external_id, status, last_event_id, parent_external_id, last_provider_ts, direction, amount, currency }
    })
}

pub struct AuditRow {
    pub entity_id: Option<uuid::Uuid>,
    pub external_id: Option<String>,
    pub event_id: Option<String>,
    pub action: String,
    pub detail: serde_json::Value,
}

pub async fn get_audit_entries(pool: &PgPool, external_id: &str) -> Vec<AuditRow> {
    sqlx::query_as::<_, (Option<uuid::Uuid>, Option<String>, Option<String>, String, serde_json::Value)>(
        "SELECT entity_id, external_id, event_id, action, detail FROM audit_log WHERE external_id = $1 ORDER BY created_at",
    )
    .bind(external_id)
    .fetch_all(pool)
    .await
    .expect("query failed")
    .into_iter()
    .map(|(entity_id, external_id, event_id, action, detail)| {
        AuditRow { entity_id, external_id, event_id, action, detail }
    })
    .collect()
}

pub async fn count_audit_entries(pool: &PgPool, external_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM audit_log WHERE external_id = $1")
        .bind(external_id)
        .fetch_one(pool)
        .await
        .expect("count failed")
}

pub async fn count_payments(pool: &PgPool, external_id: &str) -> i64 {
    sqlx::query_scalar::<_, i64>("SELECT COUNT(*) FROM payments WHERE external_id = $1")
        .bind(external_id)
        .fetch_one(pool)
        .await
        .expect("count failed")
}
