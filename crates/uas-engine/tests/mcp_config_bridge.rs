// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for agent harness MCP configuration bridge (spec 0063).

use std::fs;
use std::path::PathBuf;

use uas_domain::{Provider, SlotId};
use uas_engine::{
    AgentRole, EngineError, HarnessCommandBuilder, McpConfigBuilder, McpServerConfig,
};

#[test]
fn test_synthesized_mcp_config_schema() {
    let scratch_dir = std::env::temp_dir().join(format!("uas-mcp-test-{}", std::process::id()));
    let _ = fs::remove_dir_all(&scratch_dir);
    fs::create_dir_all(&scratch_dir).unwrap();

    let mut builder = McpConfigBuilder::new(AgentRole::Builder);
    builder.add_stdio_server("node_tool", "node", vec!["server.js"], false);
    builder.add_reverie_uds("/tmp/uas-reverie-proxy/slot-1.sock", false);

    let config_path = builder.write_to_scratch(&scratch_dir).unwrap();
    assert_eq!(config_path, scratch_dir.join("mcp-config.json"));
    assert!(config_path.exists());

    let content = fs::read_to_string(&config_path).unwrap();
    let val: serde_json::Value = serde_json::from_str(&content).unwrap();

    // Standard schema verification (REQ-0063-01, REQ-0063-02)
    assert!(val.is_object());
    let mcp_servers = val.get("mcpServers").expect("root must have mcpServers");
    assert!(mcp_servers.is_object());

    let node_server = mcp_servers.get("node_tool").unwrap();
    assert_eq!(node_server.get("command").unwrap(), "node");
    assert_eq!(
        node_server.get("args").unwrap(),
        &serde_json::json!(["server.js"])
    );

    let reverie_server = mcp_servers.get("reverie").unwrap();
    assert_eq!(reverie_server.get("command").unwrap(), "uas-bridge");
    assert_eq!(
        reverie_server.get("args").unwrap(),
        &serde_json::json!(["--socket", "/tmp/uas-reverie-proxy/slot-1.sock"])
    );

    let _ = fs::remove_dir_all(&scratch_dir);
}

#[test]
fn test_claude_invocation_includes_strict_mcp_config() {
    let slot_path = PathBuf::from("/tmp/uas-slot-claude");
    let mcp_config = slot_path.join("scratch/mcp-config.json");

    let mut builder = HarnessCommandBuilder::new(Provider::Claude, &slot_path);
    builder.with_mcp_config(&mcp_config);

    let inv = builder.build("fix failing test").unwrap();
    assert_eq!(inv.program, PathBuf::from("claude"));

    // Verify --strict-mcp-config and --mcp-config are passed (REQ-0063-03, REQ-0063-04)
    let strict_pos = inv
        .args
        .iter()
        .position(|a| a == "--strict-mcp-config")
        .expect("must contain --strict-mcp-config");
    let config_pos = inv
        .args
        .iter()
        .position(|a| a == "--mcp-config")
        .expect("must contain --mcp-config");

    assert!(strict_pos > 0);
    assert_eq!(config_pos, strict_pos + 1);
    assert_eq!(
        inv.args.get(config_pos + 1).unwrap(),
        &mcp_config.to_string_lossy().to_string()
    );
}

#[test]
fn test_reverie_uds_endpoints_bridged_to_socket() {
    let slot_id = SlotId::new("slot-worker-7").unwrap();
    let mut builder = McpConfigBuilder::new(AgentRole::Builder);
    builder.add_reverie_proxy(&slot_id, true);

    let config = builder.build().unwrap();
    let reverie = config
        .mcp_servers
        .get("reverie")
        .expect("reverie must be present");

    // REQ-0063-05: Bridge stdio to slot Unix domain socket via uas-bridge
    assert_eq!(reverie.command, "uas-bridge");
    assert_eq!(
        reverie.args,
        vec!["--socket", "/tmp/uas-reverie-proxy/slot-worker-7.sock"]
    );
}

#[test]
fn test_critic_roles_strictly_receive_read_only_capabilities() {
    let mut critic_builder = McpConfigBuilder::critic();
    critic_builder
        .add_stdio_server("file_reader", "python3", vec!["reader.py"], true)
        .add_stdio_server("file_writer", "python3", vec!["writer.py"], false);

    let critic_cfg = critic_builder.build().unwrap();

    // REQ-0063-06: Critic only receives read-only capabilities
    assert!(critic_cfg.mcp_servers.contains_key("file_reader"));
    assert!(!critic_cfg.mcp_servers.contains_key("file_writer"));

    // Builder receives both read-only and read-write
    let mut builder_cfg = McpConfigBuilder::builder();
    builder_cfg
        .add_stdio_server("file_reader", "python3", vec!["reader.py"], true)
        .add_stdio_server("file_writer", "python3", vec!["writer.py"], false);

    let b_cfg = builder_cfg.build().unwrap();
    assert!(b_cfg.mcp_servers.contains_key("file_reader"));
    assert!(b_cfg.mcp_servers.contains_key("file_writer"));
}

#[test]
fn test_unauthorized_server_commands_rejected_fail_closed() {
    let mut builder = McpConfigBuilder::new(AgentRole::Builder);
    builder.add_server(
        "rogue_tool",
        McpServerConfig::stdio("bash", vec!["-c", "rm -rf /"]),
    );

    // REQ-0063-07: Unapproved server commands rejected fail-closed
    let err = builder.build().unwrap_err();
    match err {
        EngineError::McpServerUnauthorized { server, command } => {
            assert_eq!(server, "rogue_tool");
            assert_eq!(command, "bash");
        }
        other => panic!("expected McpServerUnauthorized, got: {other:?}"),
    }
}

#[test]
fn test_injected_server_environment_variables_scrubbed_of_secrets() {
    let mut builder = McpConfigBuilder::new(AgentRole::Builder);
    let server_cfg = McpServerConfig::stdio("node", vec!["tool.js"])
        .with_env("ALLOW_DIR", "/workspace")
        .with_env("GITHUB_TOKEN", "dummy-token-sample")
        .with_env("OPENAI_API_KEY", "dummy-key-sample")
        .with_env("AUTH_SECRET", "dummy-secret-sample");

    builder.add_server("safe_tool", server_cfg);

    let root = builder.build().unwrap();
    let safe_tool = root.mcp_servers.get("safe_tool").unwrap();

    // REQ-0063-08: Secret-named variables scrubbed
    assert_eq!(safe_tool.env.get("ALLOW_DIR").unwrap(), "/workspace");
    assert!(!safe_tool.env.contains_key("GITHUB_TOKEN"));
    assert!(!safe_tool.env.contains_key("OPENAI_API_KEY"));
    assert!(!safe_tool.env.contains_key("AUTH_SECRET"));
}
