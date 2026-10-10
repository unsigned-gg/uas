//! CER-2723: Worker slot allocation state machine, capacity limits, and auto-suspend integration tests.

use std::thread;
use std::time::{Duration, Instant};

use uas_domain::{
    Budget, DomainError, Lease, LeaseId, LeaseToken, PrincipalRef, SlotId, SlotState, WorktreeRef,
};
use uas_pool::{PoolConfig, PoolError, SlotPool};

fn make_lease(
    lease_id: &str,
    slot_id: &str,
    token_str: &str,
    issued_at: u64,
    expires_at: u64,
) -> Lease {
    Lease::new(
        LeaseId::new(lease_id).expect("lease id"),
        SlotId::new(slot_id).expect("slot id"),
        PrincipalRef::new("user:tester").expect("principal ref"),
        LeaseToken::new(token_str).expect("lease token"),
        issued_at,
        expires_at,
        Budget::new(1000, 3600).expect("budget"),
    )
    .expect("valid lease")
}

// ---------------------------------------------------------------------------
// Capacity Ceiling (Spec 0015 D1, CER-2723)
// ---------------------------------------------------------------------------

#[test]
fn slot_pool_enforces_capacity_ceiling() {
    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: None,
        cgroup_base: None,
    };
    let pool = SlotPool::new(config);

    let s1 = SlotId::new("slot-01").unwrap();
    let s2 = SlotId::new("slot-02").unwrap();
    let s3 = SlotId::new("slot-03").unwrap();

    assert_eq!(pool.slot_count(), 0);

    pool.create_slot(s1.clone()).expect("create s1");
    assert_eq!(pool.slot_count(), 1);

    pool.create_slot(s2.clone()).expect("create s2");
    assert_eq!(pool.slot_count(), 2);

    // Exceeding capacity limit (max_slots = 2) is rejected
    let err = pool.create_slot(s3).unwrap_err();
    assert!(
        matches!(err, PoolError::CapacityExhausted(2)),
        "expected CapacityExhausted(2), got {:?}",
        err
    );

    // Duplicate registration is rejected
    let dup_err = pool.create_slot(s1).unwrap_err();
    assert!(matches!(dup_err, PoolError::SlotExists(_)));
}

// ---------------------------------------------------------------------------
// Slot Warming Lifecycle (Cold -> Warm)
// ---------------------------------------------------------------------------

#[test]
fn slot_warming_transitions_cold_to_warm() {
    let pool = SlotPool::new(PoolConfig::default());
    let s1 = SlotId::new("slot-warm-test").unwrap();
    pool.create_slot(s1.clone()).unwrap();

    // Cold initially
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Cold);
    assert_eq!(pool.slot_worktree(&s1).unwrap(), None);

    // Attempting to lease a cold slot fails
    let lease = make_lease("l-1", "slot-warm-test", "tok-1", 100, 200);
    let err = pool.acquire_lease(lease.clone()).unwrap_err();
    assert!(matches!(
        err,
        PoolError::InvalidSlotState(_, SlotState::Cold, "lease")
    ));

    // Warm slot with worktree reference
    let wt = WorktreeRef::new("wt-abc1234").unwrap();
    pool.warm_slot(&s1, wt.clone()).expect("warm slot");

    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Warm);
    assert_eq!(pool.slot_worktree(&s1).unwrap(), Some(wt));
}

// ---------------------------------------------------------------------------
// Lease Acquisition and Concurrency Invariant (Spec 0007 R1)
// ---------------------------------------------------------------------------

#[test]
fn lease_acquisition_and_spec_0007_r1_occupied_check() {
    let pool = SlotPool::new(PoolConfig::default());
    let s1 = SlotId::new("slot-lease-test").unwrap();
    pool.create_slot(s1.clone()).unwrap();
    pool.warm_slot(&s1, WorktreeRef::new("wt-01").unwrap())
        .unwrap();

    let lease1 = make_lease("l-01", "slot-lease-test", "secret-token-1", 1000, 2000);
    let acquired = pool.acquire_lease(lease1).expect("acquire lease 1");
    assert_eq!(acquired, s1);
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Leased);

    // Spec 0007 R1: cannot acquire another lease while active lease has not expired
    let lease2 = make_lease("l-02", "slot-lease-test", "secret-token-2", 1500, 2500);
    let r1_err = pool.acquire_lease(lease2).unwrap_err();
    assert!(
        matches!(r1_err, PoolError::Domain(DomainError::SlotLeased { .. })),
        "expected SlotLeased error under R1, got {:?}",
        r1_err
    );
}

// ---------------------------------------------------------------------------
// Lease Renewal Rules (Spec 0007 R2, R3)
// ---------------------------------------------------------------------------

#[test]
fn lease_renewal_token_verification_and_extension_rules() {
    let pool = SlotPool::new(PoolConfig::default());
    let s1 = SlotId::new("slot-renew-test").unwrap();
    pool.create_slot(s1.clone()).unwrap();
    pool.warm_slot(&s1, WorktreeRef::new("wt-02").unwrap())
        .unwrap();

    let lease = make_lease("l-renew", "slot-renew-test", "correct-token", 1000, 2000);
    pool.acquire_lease(lease).unwrap();

    let wrong_token = LeaseToken::new("wrong-token").unwrap();
    let correct_token = LeaseToken::new("correct-token").unwrap();

    // Spec 0007 R2: token must match
    let err_r2 = pool.renew_lease(&s1, &wrong_token, 1500, 60).unwrap_err();
    assert!(matches!(err_r2, PoolError::TokenMismatch(_)));

    // Spec 0007 R3: extend_by must be >= 1 second
    let err_r3 = pool.renew_lease(&s1, &correct_token, 1500, 0).unwrap_err();
    assert!(matches!(
        err_r3,
        PoolError::Domain(DomainError::Invalid { .. })
    ));

    // Expired lease cannot be renewed
    let err_expired = pool.renew_lease(&s1, &correct_token, 2500, 60).unwrap_err();
    assert!(matches!(err_expired, PoolError::LeaseExpired(_)));

    // Valid renewal
    pool.renew_lease(&s1, &correct_token, 1500, 300)
        .expect("valid renewal");
}

// ---------------------------------------------------------------------------
// Inactivity Auto-Suspend and Sweep
// ---------------------------------------------------------------------------

#[test]
fn sweep_auto_suspends_idle_slots_and_flags_expired_leases() {
    let config = PoolConfig {
        max_slots: 2,
        idle_suspend_duration: Some(Duration::from_millis(50)),
        cgroup_base: None,
    };
    let pool = SlotPool::new(config);

    let s1 = SlotId::new("slot-auto-suspend").unwrap();
    pool.create_slot(s1.clone()).unwrap();
    pool.warm_slot(&s1, WorktreeRef::new("wt-suspend").unwrap())
        .unwrap();

    let lease = make_lease("l-suspend", "slot-auto-suspend", "tok", 1000, 2000);
    pool.acquire_lease(lease).unwrap();

    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Leased);

    // Immediate sweep: slot is active, not expired
    let report1 = pool.sweep(1100, Instant::now()).expect("sweep 1");
    assert!(report1.suspended.is_empty());
    assert!(report1.expired.is_empty());

    // Wait past inactivity threshold (50ms)
    thread::sleep(Duration::from_millis(60));

    // Sweep: slot should be auto-suspended
    let report2 = pool.sweep(1100, Instant::now()).expect("sweep 2");
    assert_eq!(report2.suspended, vec![s1.clone()]);
    assert!(report2.expired.is_empty());
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Suspended);

    // Sweep with epoch past lease expiry (2000)
    let report3 = pool.sweep(2500, Instant::now()).expect("sweep 3");
    assert_eq!(report3.expired, vec![s1.clone()]);
}

// ---------------------------------------------------------------------------
// Resumption and Activity Recording
// ---------------------------------------------------------------------------

#[test]
fn slot_resumption_via_record_activity_or_explicit_resume() {
    let pool = SlotPool::new(PoolConfig::default());
    let s1 = SlotId::new("slot-resume-test").unwrap();
    pool.create_slot(s1.clone()).unwrap();
    pool.warm_slot(&s1, WorktreeRef::new("wt-res").unwrap())
        .unwrap();

    let lease = make_lease("l-res", "slot-resume-test", "tok-res", 100, 500);
    pool.acquire_lease(lease).unwrap();

    // Explicit suspend
    pool.suspend_slot(&s1).expect("suspend slot");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Suspended);

    // Record activity resumes the slot
    pool.record_activity(&s1).expect("record activity");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Leased);

    // Suspend again, test explicit resume_slot
    pool.suspend_slot(&s1).expect("suspend slot 2");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Suspended);

    pool.resume_slot(&s1).expect("explicit resume");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Leased);
}

// ---------------------------------------------------------------------------
// Release Lease and Slot Drain (Spec 0007 R4)
// ---------------------------------------------------------------------------

#[test]
fn lease_release_and_draining_retains_worktree_reference() {
    let pool = SlotPool::new(PoolConfig::default());
    let s1 = SlotId::new("slot-release-test").unwrap();
    pool.create_slot(s1.clone()).unwrap();
    let wt = WorktreeRef::new("wt-rel").unwrap();
    pool.warm_slot(&s1, wt.clone()).unwrap();

    let lease = make_lease("l-rel", "slot-release-test", "tok-rel", 100, 500);
    pool.acquire_lease(lease).unwrap();

    let wrong_tok = LeaseToken::new("wrong").unwrap();
    let correct_tok = LeaseToken::new("tok-rel").unwrap();

    // Release with wrong token fails
    assert!(matches!(
        pool.release_lease(&s1, &wrong_tok).unwrap_err(),
        PoolError::TokenMismatch(_)
    ));

    // Release with correct token returns slot to Warm
    pool.release_lease(&s1, &correct_tok)
        .expect("release lease");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Warm);

    // Spec 0007 R4: draining a slot retains its worktree reference
    pool.drain_slot(&s1).expect("drain slot");
    assert_eq!(pool.slot_state(&s1).unwrap(), SlotState::Draining);
    assert_eq!(pool.slot_worktree(&s1).unwrap(), Some(wt));
}
