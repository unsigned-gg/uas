// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for Spec 0030: A2A multi-agent interop orchestrator nodes.
//!
//! Covers:
//! - REQ-0030-01: Pure state machine types for `LlmAgent`, `Sequential`, `Parallel`, and `Loop`.
//! - REQ-0030-05: `AgentTool` capability delegation.
//! - REQ-0030-08: Maximum delegation depth of 5 rejection.

use uas_domain::{
    AgentTool, DomainError, LlmAgent, OrchestratorNode, Provider, MAX_DELEGATION_DEPTH,
};

#[test]
fn test_agent_tool_construction_and_validation() {
    let tool = AgentTool::new(
        "search_codebase",
        "Search codebase for symbols",
        "peer://cortex/indexer",
    )
    .expect("valid agent tool");

    assert_eq!(tool.name, "search_codebase");
    assert_eq!(tool.description, "Search codebase for symbols");
    assert_eq!(tool.target, "peer://cortex/indexer");

    // Empty name rejected
    let err = AgentTool::new("", "desc", "target").unwrap_err();
    assert!(matches!(
        err,
        DomainError::Invalid {
            kind: "agent_tool_name",
            ..
        }
    ));

    // Empty target rejected
    let err = AgentTool::new("name", "desc", "   ").unwrap_err();
    assert!(matches!(
        err,
        DomainError::Invalid {
            kind: "agent_tool_target",
            ..
        }
    ));
}

#[test]
fn test_llm_agent_node_construction() {
    let tool = AgentTool::new("probe", "system probe", "peer://agent/probe").unwrap();
    let agent = LlmAgent::new(
        "builder-01",
        Provider::Claude,
        "claude-3-7-sonnet",
        "You are a systems builder.",
        vec![tool],
    )
    .expect("valid agent");

    assert_eq!(agent.id, "builder-01");
    assert_eq!(agent.provider, Provider::Claude);
    assert_eq!(agent.tools.len(), 1);

    // Empty agent id rejected
    let err = LlmAgent::new("", Provider::Mock, "mock", "prompt", vec![]).unwrap_err();
    assert!(matches!(
        err,
        DomainError::Invalid {
            kind: "agent_id",
            ..
        }
    ));
}

#[test]
fn test_sequential_parallel_loop_composition() {
    let agent_a = OrchestratorNode::agent(
        LlmAgent::new("agent-a", Provider::Mock, "mock", "prompt a", vec![]).unwrap(),
    );
    let agent_b = OrchestratorNode::agent(
        LlmAgent::new("agent-b", Provider::Mock, "mock", "prompt b", vec![]).unwrap(),
    );

    // Sequential chain of 2 agents (depth = 2)
    let seq = OrchestratorNode::sequential(vec![agent_a.clone(), agent_b.clone()]);
    assert_eq!(seq.depth(), 2);
    seq.validate().expect("sequential depth 2 is valid");

    // Parallel branch of 2 agents (depth = 2)
    let par = OrchestratorNode::parallel(vec![agent_a.clone(), agent_b.clone()]);
    assert_eq!(par.depth(), 2);
    par.validate().expect("parallel depth 2 is valid");

    // Loop over sequential chain (depth = 1 + 2 = 3)
    let loop_node = OrchestratorNode::loop_node(seq, 10);
    assert_eq!(loop_node.depth(), 3);
    loop_node.validate().expect("loop depth 3 is valid");
}

#[test]
fn test_ast_delegation_depth_within_limit_passes() {
    // Construct AST with exact depth 5:
    // Level 1: Loop
    // Level 2: Sequential
    // Level 3: Parallel
    // Level 4: Loop
    // Level 5: Agent
    let leaf = OrchestratorNode::agent(
        LlmAgent::new("leaf", Provider::Mock, "mock", "charter", vec![]).unwrap(),
    );
    assert_eq!(leaf.depth(), 1);

    let l4 = OrchestratorNode::loop_node(leaf, 3);
    assert_eq!(l4.depth(), 2);

    let l3 = OrchestratorNode::parallel(vec![l4]);
    assert_eq!(l3.depth(), 3);

    let l2 = OrchestratorNode::sequential(vec![l3]);
    assert_eq!(l2.depth(), 4);

    let l1 = OrchestratorNode::loop_node(l2, 5);
    assert_eq!(l1.depth(), 5);

    assert_eq!(MAX_DELEGATION_DEPTH, 5);
    l1.validate().expect("depth 5 must be accepted");
}

#[test]
fn test_ast_delegation_depth_exceeding_limit_fails() {
    // Construct AST with depth 6 (> MAX_DELEGATION_DEPTH of 5)
    let leaf = OrchestratorNode::agent(
        LlmAgent::new("leaf", Provider::Mock, "mock", "charter", vec![]).unwrap(),
    );
    let l5 = OrchestratorNode::loop_node(leaf, 1);
    let l4 = OrchestratorNode::parallel(vec![l5]);
    let l3 = OrchestratorNode::sequential(vec![l4]);
    let l2 = OrchestratorNode::loop_node(l3, 1);
    let root = OrchestratorNode::sequential(vec![l2]);

    assert_eq!(root.depth(), 6);

    let err = root.validate().unwrap_err();
    match err {
        DomainError::Invalid {
            kind,
            value,
            reason,
        } => {
            assert_eq!(kind, "orchestrator_ast");
            assert_eq!(value, "depth 6");
            assert!(reason.contains("exceeds maximum limit of 5"));
        }
        other => panic!("unexpected error variant: {other:?}"),
    }
}

#[test]
fn test_orchestrator_ast_serde_roundtrip() {
    let tool = AgentTool::new("code_eval", "evaluates code", "peer://eval").unwrap();
    let agent = OrchestratorNode::agent(
        LlmAgent::new("agent-1", Provider::Agy, "glm-5.2", "charter", vec![tool]).unwrap(),
    );
    let seq = OrchestratorNode::sequential(vec![agent]);
    let root = OrchestratorNode::loop_node(seq, 5);

    let json = serde_json::to_string_pretty(&root).expect("serialize AST");
    let deserialized: OrchestratorNode = serde_json::from_str(&json).expect("deserialize AST");

    assert_eq!(root, deserialized);
    assert_eq!(deserialized.depth(), 3);
}
