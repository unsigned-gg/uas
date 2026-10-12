// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Agent harness streaming event adapters and translators (spec 0061).
//!
//! Translates process output from agent harnesses into typed substrate events,
//! aggregates token usage metrics, and enforces frame size safety boundaries.

use std::io::{BufRead, Read};

use serde::{Deserialize, Serialize};

/// Maximum frame size in bytes (1 MiB).
///
/// Matches the ACP server frame size limit in spec 0016.
pub const MAX_FRAME_BYTES: usize = 1024 * 1024;

/// Cumulative token usage metrics for an agent turn.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct TokenMetrics {
    /// Number of prompt tokens processed.
    pub prompt_tokens: u64,
    /// Number of completion tokens generated.
    pub completion_tokens: u64,
    /// Number of cached prompt tokens reused.
    pub cached_tokens: u64,
}

impl TokenMetrics {
    /// Total tokens billed (prompt + completion).
    pub fn total_tokens(&self) -> u64 {
        self.prompt_tokens.saturating_add(self.completion_tokens)
    }
}

/// Structured event emitted by an agent harness process stream.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum HarnessEvent {
    /// A turn started execution.
    TurnStarted {
        /// Turn identifier.
        turn_id: String,
        /// Model name.
        model: String,
    },
    /// Incremental reasoning thought chunk.
    ThoughtDelta {
        /// Thought text fragment.
        delta: String,
    },
    /// Agent requested execution of a tool.
    ToolCallRequested {
        /// Unique tool call identifier.
        call_id: String,
        /// Tool name.
        tool_name: String,
        /// Tool input arguments as JSON.
        arguments: serde_json::Value,
    },
    /// Tool execution finished.
    ToolCallCompleted {
        /// Tool call identifier matching the request.
        call_id: String,
        /// Result of the tool execution as JSON.
        result: serde_json::Value,
        /// Tool execution duration in milliseconds.
        duration_ms: u64,
    },
    /// An execution turn finished.
    TurnCompleted {
        /// Reason execution stopped (for example "stop", "tool_use", "max_tokens").
        stop_reason: String,
        /// Cumulative token usage for this turn.
        metrics: TokenMetrics,
    },
    /// Execution error reported by the harness or stream parser.
    Error {
        /// Machine-readable error code.
        code: String,
        /// Human-readable explanation.
        message: String,
    },
}

/// Translates raw harness stdout/stderr byte streams into [`HarnessEvent`] sequences.
pub struct StreamTranslator {
    max_frame_bytes: usize,
}

impl Default for StreamTranslator {
    fn default() -> Self {
        Self::new(MAX_FRAME_BYTES)
    }
}

impl StreamTranslator {
    /// Creates a translator with a specific maximum frame size limit.
    pub fn new(max_frame_bytes: usize) -> Self {
        Self { max_frame_bytes }
    }

    /// Parses events from a buffered reader.
    ///
    /// Malformed JSON lines produce [`HarnessEvent::Error`] and do not abort the stream.
    /// Lines exceeding `max_frame_bytes` produce a `frame_too_large` error and skip to the next line.
    pub fn parse_stream<R: BufRead>(&self, mut reader: R) -> Vec<HarnessEvent> {
        let mut events = Vec::new();
        let mut line_buf = Vec::new();

        loop {
            line_buf.clear();
            let read_res = reader
                .by_ref()
                .take(self.max_frame_bytes as u64 + 1)
                .read_until(b'\n', &mut line_buf);

            match read_res {
                Ok(0) => break,
                Ok(n) => {
                    let has_newline = line_buf.last() == Some(&b'\n');
                    if has_newline {
                        line_buf.pop();
                        if line_buf.last() == Some(&b'\r') {
                            line_buf.pop();
                        }
                    } else if n > self.max_frame_bytes {
                        // Skip remaining line bytes up to next newline.
                        let _ = skip_to_next_newline(&mut reader);
                        events.push(HarnessEvent::Error {
                            code: "frame_too_large".into(),
                            message: format!(
                                "stream line exceeded maximum size of {} bytes",
                                self.max_frame_bytes
                            ),
                        });
                        continue;
                    }

                    if line_buf.is_empty() {
                        continue;
                    }

                    match serde_json::from_slice::<HarnessEvent>(&line_buf) {
                        Ok(event) => events.push(event),
                        Err(err) => {
                            let preview = String::from_utf8_lossy(&line_buf);
                            let truncated = if preview.len() > 64 {
                                format!("{}...", &preview[..64])
                            } else {
                                preview.into_owned()
                            };
                            events.push(HarnessEvent::Error {
                                code: "malformed_frame".into(),
                                message: format!("{err}: {truncated}"),
                            });
                        }
                    }
                }
                Err(err) => {
                    events.push(HarnessEvent::Error {
                        code: "io_error".into(),
                        message: err.to_string(),
                    });
                    break;
                }
            }
        }

        events
    }
}

/// Discards characters until the next newline.
fn skip_to_next_newline<R: BufRead>(reader: &mut R) -> std::io::Result<()> {
    loop {
        let (done, used) = {
            let available = reader.fill_buf()?;
            if available.is_empty() {
                return Ok(());
            }
            match available.iter().position(|b| *b == b'\n') {
                Some(i) => (true, i + 1),
                None => (false, available.len()),
            }
        };
        reader.consume(used);
        if done {
            return Ok(());
        }
    }
}

/// Deterministic mock stream generator for testing and offline execution.
pub struct MockStreamGenerator;

impl MockStreamGenerator {
    /// Generates a valid JSON-encoded multi-event sequence for a tool-use turn.
    pub fn generate_tool_turn_json(
        turn_id: &str,
        tool_name: &str,
        args: serde_json::Value,
        tool_result: serde_json::Value,
        metrics: TokenMetrics,
    ) -> String {
        let events = vec![
            HarnessEvent::TurnStarted {
                turn_id: turn_id.to_string(),
                model: "mock-model".to_string(),
            },
            HarnessEvent::ThoughtDelta {
                delta: "Executing requested plan step.".to_string(),
            },
            HarnessEvent::ToolCallRequested {
                call_id: format!("{turn_id}-call-1"),
                tool_name: tool_name.to_string(),
                arguments: args,
            },
            HarnessEvent::ToolCallCompleted {
                call_id: format!("{turn_id}-call-1"),
                result: tool_result,
                duration_ms: 12,
            },
            HarnessEvent::TurnCompleted {
                stop_reason: "stop".to_string(),
                metrics,
            },
        ];

        let mut out = String::new();
        for ev in events {
            if let Ok(line) = serde_json::to_string(&ev) {
                out.push_str(&line);
                out.push('\n');
            }
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;

    #[test]
    fn parses_valid_stream_sequence() {
        let metrics = TokenMetrics {
            prompt_tokens: 120,
            completion_tokens: 45,
            cached_tokens: 30,
        };
        let ndjson = MockStreamGenerator::generate_tool_turn_json(
            "turn-1",
            "read_file",
            serde_json::json!({"path": "Cargo.toml"}),
            serde_json::json!({"content": "[package]"}),
            metrics,
        );

        let translator = StreamTranslator::default();
        let events = translator.parse_stream(Cursor::new(ndjson));

        assert_eq!(events.len(), 5);
        assert_eq!(
            events[0],
            HarnessEvent::TurnStarted {
                turn_id: "turn-1".into(),
                model: "mock-model".into()
            }
        );
        match &events[4] {
            HarnessEvent::TurnCompleted {
                stop_reason,
                metrics: m,
            } => {
                assert_eq!(stop_reason, "stop");
                assert_eq!(m.total_tokens(), 165);
            }
            other => panic!("expected TurnCompleted, got {other:?}"),
        }
    }

    #[test]
    fn recovers_from_malformed_lines_without_aborting() {
        let input = "{\"kind\":\"turn_started\",\"turn_id\":\"t1\",\"model\":\"m\"}\n\
                     THIS IS NOT VALID JSON\n\
                     {\"kind\":\"thought_delta\",\"delta\":\"ok\"}\n";

        let translator = StreamTranslator::default();
        let events = translator.parse_stream(Cursor::new(input));

        assert_eq!(events.len(), 3);
        assert!(matches!(events[0], HarnessEvent::TurnStarted { .. }));
        assert!(
            matches!(events[1], HarnessEvent::Error { ref code, .. } if code == "malformed_frame")
        );
        assert_eq!(events[2], HarnessEvent::ThoughtDelta { delta: "ok".into() });
    }

    #[test]
    fn rejects_oversized_frames_and_recovers() {
        let translator = StreamTranslator::new(64);
        let oversized = "A".repeat(100);
        let input = format!("{oversized}\n{{\"kind\":\"thought_delta\",\"delta\":\"safe\"}}\n");

        let events = translator.parse_stream(Cursor::new(input));

        assert_eq!(events.len(), 2);
        assert!(
            matches!(events[0], HarnessEvent::Error { ref code, .. } if code == "frame_too_large")
        );
        assert_eq!(
            events[1],
            HarnessEvent::ThoughtDelta {
                delta: "safe".into()
            }
        );
    }
}
