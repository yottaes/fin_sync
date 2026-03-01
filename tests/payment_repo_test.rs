mod common;

use common::*;
use fin_sync::domain::payment::{PaymentStatus, ProcessResult};
use fin_sync::services::payment_pipeline::process_payment_event;

// ── 1. create_new_payment ──────────────────────────────────────────────────

#[tokio::test]
async fn create_new_payment() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p = make_payment("pi_create_1", "evt_c1", PaymentStatus::Pending, 1000);

    let result = process_payment_event(&pool, &p, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Created(_)));

    let row = get_payment(&pool, "pi_create_1").await.unwrap();
    assert_eq!(row.status, "pending");
    assert_eq!(row.last_event_id, "evt_c1");
    assert_eq!(row.direction, "inbound");
    assert_eq!(row.amount, 5000);
    assert_eq!(row.currency, "usd");
}

// ── 2. create_writes_audit_entry ───────────────────────────────────────────

#[tokio::test]
async fn create_writes_audit_entry() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p = make_payment("pi_audit_1", "evt_a1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p, "test").await.unwrap();

    let audits = get_audit_entries(&pool, "pi_audit_1").await;
    assert_eq!(audits.len(), 1);
    assert_eq!(audits[0].action, "created");
    assert_eq!(audits[0].event_id.as_deref(), Some("evt_a1"));
}

// ── 3. transition_pending_to_succeeded ─────────────────────────────────────

#[tokio::test]
async fn transition_pending_to_succeeded() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_trans_s", "evt_t1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_trans_s", "evt_t2", PaymentStatus::Succeeded, 2000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_trans_s").await.unwrap();
    assert_eq!(row.status, "succeeded");
    assert_eq!(row.last_event_id, "evt_t2");
}

// ── 4. transition_pending_to_failed ────────────────────────────────────────

#[tokio::test]
async fn transition_pending_to_failed() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_trans_f", "evt_tf1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_trans_f", "evt_tf2", PaymentStatus::Failed, 2000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_trans_f").await.unwrap();
    assert_eq!(row.status, "failed");
}

// ── 5. transition_pending_to_refunded ──────────────────────────────────────

#[tokio::test]
async fn transition_pending_to_refunded() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_trans_r", "evt_tr1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_trans_r", "evt_tr2", PaymentStatus::Refunded, 2000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_trans_r").await.unwrap();
    assert_eq!(row.status, "refunded");
}

// ── 6. status_change_writes_audit ──────────────────────────────────────────

#[tokio::test]
async fn status_change_writes_audit() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_sca", "evt_sca1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_sca", "evt_sca2", PaymentStatus::Succeeded, 2000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    let audits = get_audit_entries(&pool, "pi_sca").await;
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[1].action, "status_changed");
    assert_eq!(audits[1].detail["old_status"], "pending");
    assert_eq!(audits[1].detail["new_status"], "succeeded");
}

// ── 7. duplicate_event_returns_duplicate ───────────────────────────────────

#[tokio::test]
async fn duplicate_event_returns_duplicate() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_dup", "evt_dup1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    // Same event_id, different NewPayment instance
    let p2 = make_payment("pi_dup", "evt_dup1", PaymentStatus::Pending, 1000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Duplicate));
}

// ── 8. same_status_returns_stale ───────────────────────────────────────────

#[tokio::test]
async fn same_status_returns_stale() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_same", "evt_same1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_same", "evt_same2", PaymentStatus::Pending, 2000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Stale(_)));

    // No additional audit entry for same-status stale
    let count = count_audit_entries(&pool, "pi_same").await;
    assert_eq!(count, 1); // only the "created" entry
}

// ── 9. older_timestamp_returns_stale ───────────────────────────────────────

#[tokio::test]
async fn older_timestamp_returns_stale() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_old", "evt_old1", PaymentStatus::Pending, 2000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    // Older timestamp with different status
    let p2 = make_payment("pi_old", "evt_old2", PaymentStatus::Succeeded, 1000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Stale(_)));

    let row = get_payment(&pool, "pi_old").await.unwrap();
    assert_eq!(row.status, "pending"); // not updated
}

// ── 10. stale_event_writes_audit ───────────────────────────────────────────

#[tokio::test]
async fn stale_event_writes_audit() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_stale_a", "evt_sa1", PaymentStatus::Pending, 2000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    let p2 = make_payment("pi_stale_a", "evt_sa2", PaymentStatus::Succeeded, 1000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    let audits = get_audit_entries(&pool, "pi_stale_a").await;
    assert_eq!(audits.len(), 2); // "created" + "event_received" (stale)
    assert_eq!(audits[1].action, "event_received");
    assert_eq!(audits[1].detail["stale"], true);
    assert_eq!(audits[1].detail["current_status"], "pending");
    assert_eq!(audits[1].detail["incoming_status"], "succeeded");
}

// ── 11. invalid_transition_succeeded_to_pending ────────────────────────────

#[tokio::test]
async fn invalid_transition_succeeded_to_pending() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_inv1", "evt_inv1a", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();
    let p2 = make_payment("pi_inv1", "evt_inv1b", PaymentStatus::Succeeded, 2000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    let p3 = make_payment("pi_inv1", "evt_inv1c", PaymentStatus::Pending, 3000);
    let result = process_payment_event(&pool, &p3, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Anomaly(_)));
}

// ── 12. invalid_transition_failed_to_succeeded ─────────────────────────────

#[tokio::test]
async fn invalid_transition_failed_to_succeeded() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_inv2", "evt_inv2a", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();
    let p2 = make_payment("pi_inv2", "evt_inv2b", PaymentStatus::Failed, 2000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    let p3 = make_payment("pi_inv2", "evt_inv2c", PaymentStatus::Succeeded, 3000);
    let result = process_payment_event(&pool, &p3, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Anomaly(_)));
}

// ── 13. anomaly_writes_audit ───────────────────────────────────────────────

#[tokio::test]
async fn anomaly_writes_audit() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_anom", "evt_anom1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();
    let p2 = make_payment("pi_anom", "evt_anom2", PaymentStatus::Succeeded, 2000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    let p3 = make_payment("pi_anom", "evt_anom3", PaymentStatus::Pending, 3000);
    process_payment_event(&pool, &p3, "test").await.unwrap();

    let audits = get_audit_entries(&pool, "pi_anom").await;
    // "created" + "status_changed" + "event_received" (anomaly)
    assert_eq!(audits.len(), 3);
    assert_eq!(audits[2].action, "event_received");
    assert_eq!(audits[2].detail["anomaly"], true);
}

// ── 14. anomaly_updates_tracking_fields ────────────────────────────────────

#[tokio::test]
async fn anomaly_updates_tracking_fields() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_track", "evt_track1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();
    let p2 = make_payment("pi_track", "evt_track2", PaymentStatus::Succeeded, 2000);
    process_payment_event(&pool, &p2, "test").await.unwrap();

    // Anomaly: Succeeded → Pending at ts=3000
    let p3 = make_payment("pi_track", "evt_track3", PaymentStatus::Pending, 3000);
    process_payment_event(&pool, &p3, "test").await.unwrap();

    let row = get_payment(&pool, "pi_track").await.unwrap();
    // Status stays succeeded (anomaly doesn't change status)
    assert_eq!(row.status, "succeeded");
    // But tracking fields advance
    assert_eq!(row.last_event_id, "evt_track3");
    assert_eq!(row.last_provider_ts, 3000);
}

// ── 15. equal_timestamp_falls_through_to_state_machine ─────────────────────

#[tokio::test]
async fn equal_timestamp_falls_through_to_state_machine() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let p1 = make_payment("pi_eq_ts", "evt_eq1", PaymentStatus::Pending, 1000);
    process_payment_event(&pool, &p1, "test").await.unwrap();

    // Same timestamp, valid transition — should succeed (strict < semantics)
    let p2 = make_payment("pi_eq_ts", "evt_eq2", PaymentStatus::Succeeded, 1000);
    let result = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(result, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_eq_ts").await.unwrap();
    assert_eq!(row.status, "succeeded");
}

// ── 16. full_lifecycle_pending_succeeded ────────────────────────────────────

#[tokio::test]
async fn full_lifecycle_pending_succeeded() {
    let pool = setup_pool("fin_sync_test_payment").await;

    let p1 = make_payment("pi_lc_s", "evt_lcs1", PaymentStatus::Pending, 1000);
    let r1 = process_payment_event(&pool, &p1, "test").await.unwrap();
    assert!(matches!(r1, ProcessResult::Created(_)));

    let p2 = make_payment("pi_lc_s", "evt_lcs2", PaymentStatus::Succeeded, 2000);
    let r2 = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(r2, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_lc_s").await.unwrap();
    assert_eq!(row.status, "succeeded");
    assert_eq!(row.last_event_id, "evt_lcs2");
    assert_eq!(row.last_provider_ts, 2000);

    let audits = get_audit_entries(&pool, "pi_lc_s").await;
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].action, "created");
    assert_eq!(audits[1].action, "status_changed");
}

// ── 17. full_lifecycle_pending_failed ──────────────────────────────────────

#[tokio::test]
async fn full_lifecycle_pending_failed() {
    let pool = setup_pool("fin_sync_test_payment").await;

    let p1 = make_payment("pi_lc_f", "evt_lcf1", PaymentStatus::Pending, 1000);
    let r1 = process_payment_event(&pool, &p1, "test").await.unwrap();
    assert!(matches!(r1, ProcessResult::Created(_)));

    let p2 = make_payment("pi_lc_f", "evt_lcf2", PaymentStatus::Failed, 2000);
    let r2 = process_payment_event(&pool, &p2, "test").await.unwrap();
    assert!(matches!(r2, ProcessResult::Updated(_)));

    let row = get_payment(&pool, "pi_lc_f").await.unwrap();
    assert_eq!(row.status, "failed");

    let audits = get_audit_entries(&pool, "pi_lc_f").await;
    assert_eq!(audits.len(), 2);
    assert_eq!(audits[0].action, "created");
    assert_eq!(audits[1].action, "status_changed");
    assert_eq!(audits[1].detail["old_status"], "pending");
    assert_eq!(audits[1].detail["new_status"], "failed");
}

// ── 18. refund_stores_parent_external_id ───────────────────────────────────

#[tokio::test]
async fn refund_stores_parent_external_id() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let r = make_refund(
        "re_parent",
        "evt_rp1",
        PaymentStatus::Pending,
        1000,
        "pi_parent_123",
    );
    process_payment_event(&pool, &r, "test").await.unwrap();

    let row = get_payment(&pool, "re_parent").await.unwrap();
    assert_eq!(row.parent_external_id.as_deref(), Some("pi_parent_123"));
    assert_eq!(row.direction, "outbound");
}

// ── 19. check_constraint_rejects_invalid_status ────────────────────────────

#[tokio::test]
async fn check_constraint_rejects_invalid_status() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let result = sqlx::query(
        r#"
        INSERT INTO payments
            (id, external_id, source, event_type, direction, amount, currency,
             status, metadata, raw_event, last_event_id, last_provider_ts)
        VALUES (gen_random_uuid(), 'pi_bad_status', 'stripe', 'test', 'inbound',
                1000, 'usd', 'cancelled', '{}', '{}', 'evt_x', 1000)
        "#,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("chk_payments_status"),
        "expected check constraint violation, got: {err}"
    );
}

// ── 20. check_constraint_rejects_negative_amount ───────────────────────────

#[tokio::test]
async fn check_constraint_rejects_negative_amount() {
    let pool = setup_pool("fin_sync_test_payment").await;
    let result = sqlx::query(
        r#"
        INSERT INTO payments
            (id, external_id, source, event_type, direction, amount, currency,
             status, metadata, raw_event, last_event_id, last_provider_ts)
        VALUES (gen_random_uuid(), 'pi_neg_amt', 'stripe', 'test', 'inbound',
                -100, 'usd', 'pending', '{}', '{}', 'evt_x', 1000)
        "#,
    )
    .execute(&pool)
    .await;

    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("chk_payments_amount"),
        "expected check constraint violation, got: {err}"
    );
}
