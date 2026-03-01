mod common;

use common::*;
use fin_sync::domain::id::{EventId, ExternalId};
use fin_sync::domain::payment::{PassthroughEvent, PaymentStatus, ProcessResult};
use fin_sync::services::payment_pipeline::{handle_passthrough, process_payment_event};

// ── 26. concurrent_duplicate_events ────────────────────────────────────────
// 10 tasks send the same event_id. Exactly 1 should get Created, rest Duplicate.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_duplicate_events() {
    let pool = setup_pool("fin_sync_test_concurrency").await;

    let mut handles = Vec::new();
    for i in 0..10 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let p = make_payment("pi_cdup", "evt_cdup_same", PaymentStatus::Pending, 1000 + i);
            process_payment_event(&pool, &p, "test").await.unwrap()
        }));
    }

    let mut created = 0;
    let mut duplicates = 0;
    for h in handles {
        match h.await.unwrap() {
            ProcessResult::Created(_) => created += 1,
            ProcessResult::Duplicate => duplicates += 1,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    assert_eq!(created, 1, "exactly 1 Created");
    assert_eq!(duplicates, 9, "9 Duplicates");
    assert_eq!(count_payments(&pool, "pi_cdup").await, 1);
}

// ── 27. concurrent_updates_same_external_id ────────────────────────────────
// First create a pending payment, then fire 5 concurrent "succeeded" events
// with different event_ids. Advisory lock serializes: 1 Updated, 4 Stale.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_updates_same_external_id() {
    let pool = setup_pool("fin_sync_test_concurrency").await;

    let p = make_payment("pi_cser", "evt_cser_init", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p, "test").await.unwrap();

    let mut handles = Vec::new();
    for i in 0..5 {
        let pool = pool.clone();
        let evt = format!("evt_cser_{i}");
        handles.push(tokio::spawn(async move {
            let p = make_payment("pi_cser", &evt, PaymentStatus::Succeeded, 2000 + i);
            process_payment_event(&pool, &p, "test").await.unwrap()
        }));
    }

    let mut updated = 0;
    let mut stale = 0;
    let mut anomaly = 0;
    for h in handles {
        match h.await.unwrap() {
            ProcessResult::Updated(_) => updated += 1,
            ProcessResult::Stale(_) => stale += 1,
            ProcessResult::Anomaly(_) => anomaly += 1,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    assert_eq!(updated, 1, "exactly 1 Updated");
    assert_eq!(stale + anomaly, 4, "4 Stale or Anomaly (all non-Updated)");

    let row = get_payment(&pool, "pi_cser").await.unwrap();
    assert_eq!(row.status, "succeeded");
}

// ── 28. concurrent_passthrough_dedup ───────────────────────────────────────
// 10 tasks with same event_id — only 1 should return true.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn concurrent_passthrough_dedup() {
    let pool = setup_pool("fin_sync_test_concurrency").await;

    let mut handles = Vec::new();
    for _ in 0..10 {
        let pool = pool.clone();
        handles.push(tokio::spawn(async move {
            let event = PassthroughEvent {
                external_id: Some(ExternalId::new("pi_cpt").unwrap()),
                event_id: EventId::new("evt_cpt_same").unwrap(),
                event_type: "charge.created".into(),
                provider_ts: 1000,
                raw_payload: serde_json::json!({"type": "charge.created"}),
                actor: "test".into(),
            };
            handle_passthrough(&pool, &event).await.unwrap()
        }));
    }

    let mut logged = 0;
    let mut dupes = 0;
    for h in handles {
        if h.await.unwrap() {
            logged += 1;
        } else {
            dupes += 1;
        }
    }

    assert_eq!(logged, 1, "exactly 1 logged");
    assert_eq!(dupes, 9, "9 duplicates");
}

// ── 29. advisory_lock_prevents_double_insert ───────────────────────────────
// 2 tasks try to create the same external_id (different event_ids).
// Advisory lock means one inserts, the other sees existing row.

#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn advisory_lock_prevents_double_insert() {
    let pool = setup_pool("fin_sync_test_concurrency").await;

    let mut handles = Vec::new();
    for i in 0..2 {
        let pool = pool.clone();
        let evt = format!("evt_adv_{i}");
        handles.push(tokio::spawn(async move {
            let p = make_payment("pi_adv_lock", &evt, PaymentStatus::Pending, 1000 + i);
            process_payment_event(&pool, &p, "test").await.unwrap()
        }));
    }

    let mut created = 0;
    let mut stale = 0;
    for h in handles {
        match h.await.unwrap() {
            ProcessResult::Created(_) => created += 1,
            ProcessResult::Stale(_) => stale += 1,
            other => panic!("unexpected result: {other:?}"),
        }
    }

    assert_eq!(created, 1, "exactly 1 Created");
    assert_eq!(stale, 1, "exactly 1 Stale (same status)");
    assert_eq!(
        count_payments(&pool, "pi_adv_lock").await,
        1,
        "exactly 1 row"
    );
}
