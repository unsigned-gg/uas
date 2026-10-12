// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Integration tests for agent harness stream translation and token accounting (spec 0061).

use std::io::Cursor;

use uas_engine::{
    HarnessEvent, MockStreamGenerator, StreamTranslator, TokenMetrics, MAX_HARNESS_FRAME_BYTES,
};

#[test]
fn test_harness_stream_roundtrip_and_metrics() {
    let metrics = TokenMetrics {
        prompt_tokens: 420,
        completion_tokens: 88,
        cached_tokens: 150,
    };
    let ndjson = MockStreamGenerator::generate_tool_turn_json(
        "turn-exec-42",
        "bash",
        serde_json::json!({"command": "cargo test -j 2"}),
        serde_json::json!({"exit_code": 0, "output": "test result: ok"}),
        metrics,
    );

    let translator = StreamTranslator::default();
    let events = translator.parse_stream(Cursor::new(ndjson));

    assert_eq!(events.len(), 5);
    assert_eq!(
        events[0],
        HarnessEvent::TurnStarted {
            turn_id: "turn-exec-42".into(),
            model: "mock-model".into(),
        }
    );
    assert_eq!(
        events[1],
        HarnessEvent::ThoughtDelta {
            delta: "Executing requested plan step.".into(),
        }
    );
    assert!(
        matches!(&events[2], HarnessEvent::ToolCallRequested { tool_name, .. } if tool_name == "bash")
    );
    assert!(
        matches!(&events[3], HarnessEvent::ToolCallCompleted { duration_ms, .. } if *duration_ms == 12)
    );

    match &events[4] {
        HarnessEvent::TurnCompleted {
            stop_reason,
            metrics: m,
        } => {
            assert_eq!(stop_reason, "stop");
            assert_eq!(m.prompt_tokens, 420);
            assert_eq!(m.completion_tokens, 88);
            assert_eq!(m.cached_tokens, 150);
            assert_eq!(m.total_tokens(), 508);
        }
        other => panic!("expected TurnCompleted, found {other:?}"),
    }
}

#[test]
fn test_harness_stream_recovers_after_malformed_chunk() {
    let corrupted_stream = "{\"kind\":\"turn_started\",\"turn_id\":\"t-100\",\"model\":\"gpt-mock\"}\n\
                            GARBAGE_PAYLOAD_NOT_JSON_AT_ALL\n\
                            {\"kind\":\"thought_delta\",\"delta\":\"resumed after corrupt chunk\"}\n\
                            {\"kind\":\"turn_completed\",\"stop_reason\":\"stop\",\"metrics\":{\"prompt_tokens\":10,\"completion_tokens\":5,\"cached_tokens\":0}}\n";

    let translator = StreamTranslator::default();
    let events = translator.parse_stream(Cursor::new(corrupted_stream));

    assert_eq!(events.len(), 4);
    assert!(matches!(&events[0], HarnessEvent::TurnStarted { .. }));
    assert!(matches!(&events[1], HarnessEvent::Error { code, .. } if code == "malformed_frame"));
    assert_eq!(
        events[2],
        HarnessEvent::ThoughtDelta {
            delta: "resumed after corrupt chunk".into(),
        }
    );
    assert!(matches!(&events[3], HarnessEvent::TurnCompleted { .. }));
}

#[test]
fn test_harness_stream_rejects_oversized_frame() {
    let small_limit_translator = StreamTranslator::new(128);
    let giant_thought = "X".repeat(256);
    let input = format!(
        "{giant_thought}\n{{\"kind\":\"thought_delta\",\"delta\":\"recovered after giant frame\"}}\n"
    );

    let events = small_limit_translator.parse_stream(Cursor::new(input));

    assert_eq!(events.len(), 2);
    match &events[0] {
        HarnessEvent::Error { code, message } => {
            assert_eq!(code, "frame_too_large");
            assert!(message.contains("exceeded maximum size"));
        }
        other => panic!("expected frame_too_large error, got {other:?}"),
    }
    assert_eq!(
        events[1],
        HarnessEvent::ThoughtDelta {
            delta: "recovered after giant frame".into(),
        }
    );
}

#[test]
fn test_max_frame_bytes_constant_matches_acp() {
    assert_eq!(MAX_HARNESS_FRAME_BYTES, 1024 * 1024);
}
