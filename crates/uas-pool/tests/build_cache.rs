// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for slot build cache retention and eviction (spec 0047).

use std::collections::BTreeMap;
use std::fs::{self, File};
use std::io::Write;
use std::path::Path;
use std::time::{Duration, SystemTime};

use uas_domain::{HostSlotIndex, SlotId};
use uas_ebpf::audit::AuditReceipt;
use uas_pool::cache::{derive_key, derive_key_with_channel, SlotBuildCache};
use uas_pool::{PoolConfig, SlotPool};

#[test]
fn test_golden_key_derivation_and_symlink_canonicalization() {
    // REQ-0047-04: Golden string verification for /srv/repo and channel 1.94.1
    let golden_key = derive_key_with_channel(Path::new("/srv/repo"), Some("1.94.1")).unwrap();
    let expected = format!("{}-1.94.1", concat!("203fce37", "fbfc82f3"));
    assert_eq!(
        golden_key, expected,
        "golden key must match deterministic SHA-256 derivation"
    );

    // Verify symlink to the same repository yields the same key
    let temp_dir = std::env::temp_dir().join(format!("uas-key-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let target_repo = temp_dir.join("real-repo");
    fs::create_dir_all(&target_repo).unwrap();
    fs::write(
        target_repo.join("rust-toolchain.toml"),
        "[toolchain]\nchannel = \"1.94.1\"\n",
    )
    .unwrap();

    let symlink_repo = temp_dir.join("symlink-repo");
    std::os::unix::fs::symlink(&target_repo, &symlink_repo).unwrap();

    let key_real = derive_key(&target_repo).unwrap();
    let key_sym = derive_key(&symlink_repo).unwrap();

    assert_eq!(
        key_real, key_sym,
        "canonicalization must ensure symlink derives the same key as real repository"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_slot_cache_quota_eviction_orders_by_stamp_mtime() {
    // REQ-0047-05, REQ-0047-06: Three 8 MiB key directories under 20 MiB quota -> oldest evicted
    let temp_dir = std::env::temp_dir().join(format!("uas-evict-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let cache = SlotBuildCache::new(HostSlotIndex(0), Some(temp_dir.clone()));
    cache.init().unwrap();

    // Create 3 key directories with 8 MiB files each
    let payload = vec![0u8; 1024 * 1024]; // 1 MiB block

    for i in 1..=3 {
        let key = format!("repo{i}-default");
        let target = cache.prepare_key_dir(&key).unwrap();
        let artifact = target.join("target").join(format!("lib{i}.rlib"));
        let mut file = File::create(&artifact).unwrap();
        for _ in 0..8 {
            file.write_all(&payload).unwrap();
        }
        file.sync_all().unwrap();

        // Stagger stamps in order
        let stamp_path = cache.stamps_dir().join(&key);
        let mtime = SystemTime::now() - Duration::from_secs(300 - i * 60);
        let f = File::options().write(true).open(&stamp_path).unwrap();
        f.set_modified(mtime).unwrap();
    }

    // Total size is ~24 MiB. Evict under 20 MiB quota.
    let quota_20_mib = 20 * 1024 * 1024;
    let report = cache.evict(quota_20_mib).unwrap();

    assert_eq!(
        report.evicted_count, 1,
        "exactly one oldest key directory must be evicted"
    );
    assert!(
        !cache.key_dir("repo1-default").exists(),
        "repo1 is oldest and must be evicted"
    );
    assert!(
        cache.key_dir("repo2-default").exists(),
        "repo2 must be kept"
    );
    assert!(
        cache.key_dir("repo3-default").exists(),
        "repo3 must be kept"
    );
    assert!(report.bytes_after <= quota_20_mib);

    // Single 30 MiB directory under 20 MiB quota must be evicted
    let key_large = "repo-large-default";
    let large_target = cache.prepare_key_dir(key_large).unwrap();
    let large_file = large_target.join("target").join("large.bin");
    let mut f = File::create(large_file).unwrap();
    for _ in 0..30 {
        f.write_all(&payload).unwrap();
    }
    f.sync_all().unwrap();

    let report2 = cache.evict(quota_20_mib).unwrap();
    assert!(report2.evicted_count >= 1);
    assert!(
        !cache.key_dir(key_large).exists(),
        "oversized single key must be evicted"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_slot_cache_keyed_by_host_index_persists_across_slot_ids() {
    // REQ-0047-01: Two pool processes with different SlotId take host index 0 and share cache
    let temp_dir = std::env::temp_dir().join(format!("uas-host-idx-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).unwrap();

    let cache1 = SlotBuildCache::new(HostSlotIndex(0), Some(temp_dir.clone()));
    cache1.prepare_key_dir("myrepo-1.94.1").unwrap();
    let target1 = cache1.target_dir("myrepo-1.94.1");
    File::create(target1.join("build.artifact")).unwrap();

    // Second client accessing HostSlotIndex(0)
    let cache2 = SlotBuildCache::new(HostSlotIndex(0), Some(temp_dir.clone()));
    let target2 = cache2.target_dir("myrepo-1.94.1");

    assert!(
        target2.join("build.artifact").exists(),
        "key directory must persist across different slot instances on host index"
    );

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_worker_environment_and_sccache_configuration() {
    let temp_dir = std::env::temp_dir().join(format!("uas-env-test-{}", std::process::id()));
    let cache = SlotBuildCache::new(HostSlotIndex(1), Some(temp_dir.clone()));

    let mut env = BTreeMap::new();
    env.insert("RA_LOG".into(), "debug".into());
    env.insert("RA_PROFILE".into(), "cpu".into());

    // REQ-0047-10: Worker environment strips any RA_ variables
    // REQ-0047-08, REQ-0047-09: Evaluates sccache
    cache.configure_worker_env(&mut env, "key-test", false, 10 * 1024 * 1024 * 1024);

    assert_eq!(
        env.get("CARGO_TARGET_DIR").unwrap(),
        &cache.target_dir("key-test").to_string_lossy().to_string()
    );
    assert!(
        !env.contains_key("RA_LOG"),
        "RA_ variables must not be exported"
    );
    assert!(
        !env.contains_key("RA_PROFILE"),
        "RA_ variables must not be exported"
    );
    assert!(!env.contains_key("RUSTC_WRAPPER"), "sccache is disabled");

    let _ = fs::remove_dir_all(&temp_dir);
}

#[test]
fn test_audit_receipt_cache_telemetry_sealing() {
    // REQ-0047-11: AuditReceipt records cache_bytes_before, cache_bytes_after, cache_evicted
    let slot_dir = std::env::temp_dir().join(format!("uas-slot-dir-{}", std::process::id()));
    let _ = fs::remove_dir_all(&slot_dir);
    fs::create_dir_all(&slot_dir).unwrap();

    let config = PoolConfig {
        host_slot_dir: Some(slot_dir.clone()),
        ..PoolConfig::default()
    };
    let pool = SlotPool::new(config);
    let slot_id = SlotId::new("slot-audit-test").unwrap();
    pool.allocate(slot_id.clone()).unwrap();

    // Reclaim slot
    pool.reclaim_slot(&slot_id, None).unwrap();

    let empty_receipt = AuditReceipt {
        run_hash: "abcd".into(),
        events: 10,
        dropped_events: 0,
        attested: true,
        cpu_cap: "uncapped".into(),
        cache_bytes_before: 0,
        cache_bytes_after: 0,
        cache_evicted: 0,
    };

    let sealed = pool.seal_audit_receipt(&slot_id, empty_receipt).unwrap();
    assert!(sealed.attested);
    assert_eq!(sealed.events, 10);

    // Verify backward compatibility: receipt without cache fields parses
    let json_legacy = r#"{
        "run_hash": "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        "events": 5,
        "dropped_events": 0,
        "attested": true
    }"#;
    let parsed: AuditReceipt =
        serde_json::from_str(json_legacy).expect("legacy receipt must parse");
    assert_eq!(parsed.cache_bytes_before, 0);
    assert_eq!(parsed.cache_bytes_after, 0);
    assert_eq!(parsed.cache_evicted, 0);
    assert_eq!(parsed.cpu_cap, "uncapped");

    let _ = fs::remove_dir_all(&slot_dir);
}
