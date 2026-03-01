mod common;

use common::*;
use fin_sync::domain::id::{EventId, ExternalId};
use fin_sync::domain::payment::{PassthroughEvent, PaymentStatus};
use fin_sync::services::payment_pipeline::{handle_passthrough, process_payment_event};

// ── 21. passthrough_logs_event ─────────────────────────────────────────────

#[tokio::test]
async fn passthrough_logs_event() {
    let pool = setup_pool("fin_sync_test_passthrough").await;

    let event = PassthroughEvent {
        external_id: Some(ExternalId::new("pi_pt_1").unwrap()),
        event_id: EventId::new("evt_pt_1").unwrap(),
        event_type: "charge.created".into(),
        provider_ts: 1000,
        raw_payload: serde_json::json!({"type": "charge.created"}),
        actor: "test".into(),
    };
    let result = handle_passthrough(&pool, &event).await.unwrap();
    assert!(result); // new event

    // Check audit log
    let audits = get_audit_entries(&pool, "pi_pt_1").await;
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "event_received");
    assert_eq!(audits[0].detail["passthrough"], true);
    assert_eq!(audits[0].detail["event_type"], "charge.created");
}

// ── 22. passthrough_duplicate_returns_false ─────────────────────────────────

#[tokio::test]
async fn passthrough_duplicate_returns_false() {
    let pool = setup_pool("fin_sync_test_passthrough").await;

    let event = PassthroughEvent {
        external_id: Some(ExternalId::new("pi_ptd").unwrap()),
        event_id: EventId::new("evt_ptd_1").unwrap(),
        event_type: "charge.created".into(),
        provider_ts: 1000,
        raw_payload: serde_json::json!({"type": "charge.created"}),
        actor: "test".into(),
    };

    let r1 = handle_passthrough(&pool, &event).await.unwrap();
    assert!(r1);

    let r2 = handle_passthrough(&pool, &event).await.unwrap();
    assert!(!r2); // duplicate
}

// ── 23. passthrough_links_existing_payment ──────────────────────────────────

#[tokio::test]
async fn passthrough_links_existing_payment() {
    let pool = setup_pool("fin_sync_test_passthrough").await;

    // Create a payment first
    let p = make_payment(
        "pi_ptlink",
        "evt_ptlink_create",
        PaymentStatus::Pending,
        1000,
    );
    process_payment_event(&pool, &p, "test").await.unwrap();
    let payment_row = get_payment(&pool, "pi_ptlink").await.unwrap();

    // Now log a passthrough event referencing the same external_id
    let event = PassthroughEvent {
        external_id: Some(ExternalId::new("pi_ptlink").unwrap()),
        event_id: EventId::new("evt_ptlink_pt").unwrap(),
        event_type: "charge.succeeded".into(),
        provider_ts: 2000,
        raw_payload: serde_json::json!({"type": "charge.succeeded"}),
        actor: "test".into(),
    };
    handle_passthrough(&pool, &event).await.unwrap();

    // The audit entry should have entity_id pointing to the payment
    let audits: Vec<_> = sqlx::query_as::<_, (Option<uuid::Uuid>, String)>(
        "SELECT entity_id, action FROM audit_log WHERE event_id = $1",
    )
    .bind("evt_ptlink_pt")
    .fetch_all(&pool)
    .await
    .unwrap();

    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].0, Some(payment_row.id));
}

// ── 24. passthrough_no_existing_payment ─────────────────────────────────────

#[tokio::test]
async fn passthrough_no_existing_payment() {
    let pool = setup_pool("fin_sync_test_passthrough").await;

    let event = PassthroughEvent {
        external_id: Some(ExternalId::new("pi_nonexistent").unwrap()),
        event_id: EventId::new("evt_ptnone").unwrap(),
        event_type: "charge.created".into(),
        provider_ts: 1000,
        raw_payload: serde_json::json!({"type": "charge.created"}),
        actor: "test".into(),
    };
    handle_passthrough(&pool, &event).await.unwrap();

    // entity_id should be NULL since no matching payment exists
    let row: Option<(Option<uuid::Uuid>,)> =
        sqlx::query_as("SELECT entity_id FROM audit_log WHERE event_id = $1")
            .bind("evt_ptnone")
            .fetch_optional(&pool)
            .await
            .unwrap();

    assert!(row.unwrap().0.is_none());
}

// ── 25. passthrough_with_none_external_id ───────────────────────────────────

#[tokio::test]
async fn passthrough_with_none_external_id() {
    let pool = setup_pool("fin_sync_test_passthrough").await;

    let event = PassthroughEvent {
        external_id: None,
        event_id: EventId::new("evt_ptnull").unwrap(),
        event_type: "unknown.event".into(),
        provider_ts: 1000,
        raw_payload: serde_json::json!({"type": "unknown.event"}),
        actor: "test".into(),
    };
    let result = handle_passthrough(&pool, &event).await.unwrap();
    assert!(result);

    // Audit entry should have NULL external_id and NULL entity_id
    let row: Option<(Option<String>, Option<uuid::Uuid>)> =
        sqlx::query_as("SELECT external_id, entity_id FROM audit_log WHERE event_id = $1")
            .bind("evt_ptnull")
            .fetch_optional(&pool)
            .await
            .unwrap();

    let (ext_id, entity_id) = row.unwrap();
    assert!(ext_id.is_none());
    assert!(entity_id.is_none());
}
