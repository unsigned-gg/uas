// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for REQ-0017-05: replica failover and deterministic recovery.
//!
//! WHEN a chosen replica becomes unavailable, the router shall fail over to the next
//! healthy replica according to Highest Random Weight (HRW) rendezvous scoring.

use uas_router::affinity::{AffinityTable, ReplicaChoice, ReplicaId, RouterError};
use uas_router::prefix::PrefixInput;

fn make_id(name: &str) -> ReplicaId {
    ReplicaId::new(name).expect("valid replica id")
}

#[test]
fn failover_to_next_highest_weight_healthy_replica() {
    let mut table = AffinityTable::new();
    let r1 = make_id("vllm-replica-alpha");
    let r2 = make_id("vllm-replica-beta");
    let r3 = make_id("vllm-replica-gamma");

    table.insert(r1.clone());
    table.insert(r2.clone());
    table.insert(r3.clone());

    let input = PrefixInput {
        system: vec![
            "You are an assistant with strict system instructions and verified safety policies."
                .to_string(),
        ],
        tools: vec![],
    };
    let hash = input.hash();

    // Ranked list gives the deterministic preference order for this hash.
    let ranking = table.ranked(&hash);
    assert_eq!(ranking.len(), 3);
    let first = &ranking[0];
    let second = &ranking[1];
    let third = &ranking[2];

    // Initial choice selects the highest-weight replica.
    let choice1 = table.choose(&hash).expect("healthy choice");
    assert_eq!(choice1, ReplicaChoice::Affinity(first.clone()));
    assert!(!choice1.is_fallback());

    // Mark the chosen replica as unavailable (e.g. 2 consecutive health probe failures).
    table.set_healthy(first, false);

    // Unwanted behavior prevention: Router immediately fails over to the next healthy replica.
    let choice2 = table.choose(&hash).expect("failover choice");
    assert_eq!(choice2, ReplicaChoice::Affinity(second.clone()));
    assert!(!choice2.is_fallback());

    // Mark the second replica as unavailable.
    table.set_healthy(second, false);

    // Router fails over to the third healthy replica.
    let choice3 = table.choose(&hash).expect("failover choice 2");
    assert_eq!(choice3, ReplicaChoice::Affinity(third.clone()));
    assert!(!choice3.is_fallback());

    // When the primary replica recovers (3 consecutive probe successes), routing restores to it.
    table.set_healthy(first, true);
    let choice_restored = table.choose(&hash).expect("restored choice");
    assert_eq!(choice_restored, ReplicaChoice::Affinity(first.clone()));
}

#[test]
fn failover_from_request_payload_integration() {
    let mut table = AffinityTable::new();
    let r1 = make_id("replica-east-1");
    let r2 = make_id("replica-west-1");

    table.insert(r1.clone());
    table.insert(r2.clone());

    let payload = br#"{
        "model": "deepseek-coder",
        "messages": [
            {"role": "system", "content": "You are a Rust compiler specialist."},
            {"role": "user", "content": "Optimize this loop."}
        ]
    }"#;

    let input = PrefixInput::from_request(payload).expect("valid payload");
    let hash = input.hash();

    let ranking = table.ranked(&hash);
    let top = &ranking[0];
    let backup = &ranking[1];

    assert_eq!(
        table.choose(&hash).unwrap(),
        ReplicaChoice::Affinity(top.clone())
    );

    // Trip top node
    table.set_healthy(top, false);
    assert_eq!(
        table.choose(&hash).unwrap(),
        ReplicaChoice::Affinity(backup.clone())
    );
}

#[test]
fn failover_when_all_replicas_unhealthy_triggers_deterministic_fallback() {
    let mut table = AffinityTable::new();
    let r1 = make_id("node-1");
    let r2 = make_id("node-2");
    let r3 = make_id("node-3");

    table.insert(r1.clone());
    table.insert(r2.clone());
    table.insert(r3.clone());

    let input = PrefixInput {
        system: vec!["System instructions for fallback test".to_string()],
        tools: vec![],
    };
    let hash = input.hash();

    // Mark all replicas unhealthy.
    table.set_healthy(&r1, false);
    table.set_healthy(&r2, false);
    table.set_healthy(&r3, false);

    // With zero healthy replicas, choose must fall back to round-robin, never panic or error out.
    let choice = table.choose(&hash).expect("fallback choice");
    assert!(choice.is_fallback());

    // The fallback replica must be one of the registered nodes.
    let selected = choice.replica();
    assert!(selected == &r1 || selected == &r2 || selected == &r3);

    // Deterministic fallback index calculation matches spec 0017 D5:
    let mut first_8 = [0u8; 8];
    first_8.copy_from_slice(&hash.0[..8]);
    let expected_idx = (u64::from_be_bytes(first_8) % 3) as usize;
    let mut sorted_nodes = [r1, r2, r3];
    sorted_nodes.sort();
    assert_eq!(selected, &sorted_nodes[expected_idx]);
}

#[test]
fn failover_empty_table_returns_no_healthy_replicas_error() {
    let table = AffinityTable::new();
    let input = PrefixInput {
        system: vec!["Empty test".to_string()],
        tools: vec![],
    };
    let hash = input.hash();

    let err = table.choose(&hash).expect_err("must error on empty table");
    assert_eq!(err, RouterError::NoHealthyReplicas);
}
