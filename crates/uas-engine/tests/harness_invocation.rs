// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for agent harness CLI invocation and environment sanitization (spec 0062).

use std::path::PathBuf;

use uas_domain::{LeaseId, Provider, SlotId};
use uas_engine::{
    is_secret_env_key, EngineError, HarnessCommandBuilder, HarnessEnv, SessionId,
    ALLOWED_AMBIENT_ENV,
};

#[test]
fn test_environment_sanitization_scrubs_secrets_and_retains_allowlist() {
    let slot_path = PathBuf::from("/tmp/uas-slot-test");
    let builder = HarnessCommandBuilder::new(Provider::Claude, &slot_path);
    let inv = builder.build("run build").unwrap();

    // Verify ambient environment does not contain any secret keys
    for k in inv.env.keys() {
        assert!(
            !is_secret_env_key(k),
            "environment should not contain secret key: {k}"
        );
    }

    // Verify config directories are isolated to slot path
    assert_eq!(inv.env.get("HOME").unwrap(), "/tmp/uas-slot-test");
    assert_eq!(
        inv.env.get("XDG_CONFIG_HOME").unwrap(),
        "/tmp/uas-slot-test/.config"
    );
    assert_eq!(
        inv.env.get("CLAUDE_CONFIG_DIR").unwrap(),
        "/tmp/uas-slot-test/.config/claude"
    );
    assert_eq!(
        inv.env.get("AGY_CONFIG_DIR").unwrap(),
        "/tmp/uas-slot-test/.config/agy"
    );
}

#[test]
fn test_slot_identity_and_proxy_injection() {
    let slot_path = PathBuf::from("/tmp/uas-slot-worker-1");
    let slot_id = SlotId::new("slot-worker-1").unwrap();
    let lease_id = LeaseId::new("lease-8899").unwrap();
    let session_id = SessionId::new("sess-exec-1").unwrap();

    let mut builder = HarnessCommandBuilder::new(Provider::Agy, &slot_path);
    builder
        .with_slot_identity(slot_id, lease_id, session_id)
        .with_model("gemini-3.5-pro")
        .with_proxy("http://127.0.0.1:9099");

    let inv = builder.build("implement task").unwrap();

    assert_eq!(inv.env.get("UAS_SLOT_ID").unwrap(), "slot-worker-1");
    assert_eq!(inv.env.get("UAS_LEASE_ID").unwrap(), "lease-8899");
    assert_eq!(inv.env.get("UAS_SESSION_ID").unwrap(), "sess-exec-1");
    assert_eq!(inv.env.get("UAS_PROVIDER").unwrap(), "agy");
    assert_eq!(inv.env.get("UAS_MODEL").unwrap(), "gemini-3.5-pro");
    assert_eq!(inv.env.get("HTTP_PROXY").unwrap(), "http://127.0.0.1:9099");
    assert_eq!(inv.env.get("HTTPS_PROXY").unwrap(), "http://127.0.0.1:9099");
    assert_eq!(inv.env.get("ALL_PROXY").unwrap(), "http://127.0.0.1:9099");
}

#[test]
fn test_cli_argument_vectors_match_canonical_headless_spec() {
    let slot_path = PathBuf::from("/tmp/uas-slot");

    // Claude Code
    let claude_inv = HarnessCommandBuilder::new(Provider::Claude, &slot_path)
        .build("execute plan")
        .unwrap();
    assert_eq!(claude_inv.program, PathBuf::from("claude"));
    assert_eq!(
        claude_inv.args,
        vec![
            "--print",
            "--output-format",
            "json",
            "-p",
            "execute plan",
            "--dangerously-skip-permissions"
        ]
    );

    // AGY
    let agy_inv = HarnessCommandBuilder::new(Provider::Agy, &slot_path)
        .build("execute plan")
        .unwrap();
    assert_eq!(agy_inv.program, PathBuf::from("agy"));
    assert_eq!(
        agy_inv.args,
        vec![
            "run",
            "--stream",
            "--format",
            "jsonl",
            "--prompt",
            "execute plan"
        ]
    );

    // OMP
    let omp_inv = HarnessCommandBuilder::new(Provider::Omp, &slot_path)
        .build("execute plan")
        .unwrap();
    assert_eq!(omp_inv.program, PathBuf::from("omp"));
    assert_eq!(
        omp_inv.args,
        vec!["exec", "--json", "--prompt", "execute plan"]
    );

    // Codex
    let codex_inv = HarnessCommandBuilder::new(Provider::Codex, &slot_path)
        .build("execute plan")
        .unwrap();
    assert_eq!(codex_inv.program, PathBuf::from("codex"));
    assert_eq!(codex_inv.args, vec!["exec", "--json", "execute plan"]);

    // Gateway
    let gateway_inv = HarnessCommandBuilder::new(Provider::Gateway, &slot_path)
        .with_model("glm-5.2")
        .build("execute plan")
        .unwrap();
    assert_eq!(gateway_inv.program, PathBuf::from("uas-gateway-client"));
    assert_eq!(
        gateway_inv.args,
        vec!["--model", "glm-5.2", "--prompt", "execute plan"]
    );

    // Mock
    let mock_inv = HarnessCommandBuilder::new(Provider::Mock, &slot_path)
        .build("execute plan")
        .unwrap();
    assert_eq!(mock_inv.program, PathBuf::from("mock-harness"));
    assert_eq!(mock_inv.args, vec!["--task", "execute plan"]);
}

#[test]
fn test_negative_control_rejects_empty_and_null_byte_prompts() {
    let slot_path = PathBuf::from("/tmp/uas-slot");
    let builder = HarnessCommandBuilder::new(Provider::Mock, &slot_path);

    let empty_err = builder.build("   \t\n  ").unwrap_err();
    assert!(matches!(empty_err, EngineError::InvalidCommandInput { .. }));

    let null_err = builder.build("hello\0world").unwrap_err();
    assert!(matches!(null_err, EngineError::InvalidCommandInput { .. }));
}

#[test]
fn test_negative_control_rejects_secret_injection() {
    let mut env = HarnessEnv::new();
    let err = env.set("OPENAI_API_KEY", "sk-12345").unwrap_err();
    assert!(matches!(err, EngineError::PolicyViolation(_)));

    let mut builder = HarnessCommandBuilder::new(Provider::Mock, "/tmp/uas-slot");
    let builder_err = builder.with_env("ANTHROPIC_AUTH_TOKEN", "tok-xyz");
    assert!(matches!(builder_err, Err(EngineError::PolicyViolation(_))));
}

#[test]
fn test_allowed_ambient_list_contains_only_safe_system_keys() {
    for key in ALLOWED_AMBIENT_ENV {
        assert!(
            !is_secret_env_key(key),
            "allowed ambient key {key} must not look like a secret"
        );
    }
}
