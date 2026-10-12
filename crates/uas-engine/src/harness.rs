// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Agent harness streaming event adapters and translators (spec 0061).
//!
//! Translates process output from agent harnesses into typed substrate events,
//! aggregates token usage metrics, and enforces frame size safety boundaries.

use std::collections::BTreeMap;
use std::io::{BufRead, Read};
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};
use uas_domain::{LeaseId, Provider, SlotId};

use crate::{EngineError, SessionId};

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

    /// Parses events from a buffered reader using a harness-specific line parser.
    pub fn parse_with<P: HarnessStreamParser, R: BufRead>(
        &self,
        parser: &mut P,
        mut reader: R,
    ) -> Vec<HarnessEvent> {
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

                    if let Ok(line_str) = std::str::from_utf8(&line_buf) {
                        if let Some(event) = parser.parse_line(line_str) {
                            events.push(event);
                        }
                    } else {
                        events.push(HarnessEvent::Error {
                            code: "invalid_utf8".into(),
                            message: "stream frame was not valid utf-8".into(),
                        });
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

/// Trait for harness-specific line parsers.
pub trait HarnessStreamParser {
    /// Attempt to parse a raw harness stdout line into a typed [`HarnessEvent`].
    /// Returns `None` if the line is not recognized or contains non-event output.
    fn parse_line(&mut self, line: &str) -> Option<HarnessEvent>;
}

/// Claude Code CLI streaming JSON line parser.
#[derive(Debug, Default)]
pub struct ClaudeStreamParser;

impl HarnessStreamParser for ClaudeStreamParser {
    fn parse_line(&mut self, line: &str) -> Option<HarnessEvent> {
        let val: serde_json::Value = serde_json::from_str(line).ok()?;
        let typ = val.get("type")?.as_str()?;
        match typ {
            "message_start" => {
                let msg = val.get("message")?;
                let turn_id = msg.get("id")?.as_str()?.to_string();
                let model = msg
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("claude-unknown")
                    .to_string();
                Some(HarnessEvent::TurnStarted { turn_id, model })
            }
            "content_block_delta" => {
                let delta = val.get("delta")?;
                if delta.get("type")?.as_str()? == "text_delta" {
                    let text = delta.get("text")?.as_str()?.to_string();
                    Some(HarnessEvent::ThoughtDelta { delta: text })
                } else {
                    None
                }
            }
            "tool_use" => {
                let call_id = val.get("id")?.as_str()?.to_string();
                let tool_name = val.get("name")?.as_str()?.to_string();
                let arguments = val.get("input").cloned().unwrap_or(serde_json::Value::Null);
                Some(HarnessEvent::ToolCallRequested {
                    call_id,
                    tool_name,
                    arguments,
                })
            }
            "tool_result" => {
                let call_id = val.get("tool_use_id")?.as_str()?.to_string();
                let result = val
                    .get("content")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(HarnessEvent::ToolCallCompleted {
                    call_id,
                    result,
                    duration_ms: 0,
                })
            }
            "message_delta" => {
                let usage = val.get("usage");
                let completion_tokens = usage
                    .and_then(|u| u.get("output_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let prompt_tokens = usage
                    .and_then(|u| u.get("input_tokens"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let stop_reason = val
                    .get("delta")
                    .and_then(|d| d.get("stop_reason"))
                    .and_then(|v| v.as_str())
                    .unwrap_or("end_turn")
                    .to_string();
                Some(HarnessEvent::TurnCompleted {
                    stop_reason,
                    metrics: TokenMetrics {
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens: 0,
                    },
                })
            }
            _ => None,
        }
    }
}

/// Antigravity (AGY) CLI streaming JSON line parser.
#[derive(Debug, Default)]
pub struct AgyStreamParser;

impl HarnessStreamParser for AgyStreamParser {
    fn parse_line(&mut self, line: &str) -> Option<HarnessEvent> {
        let val: serde_json::Value = serde_json::from_str(line).ok()?;
        let step = val.get("step")?.as_str()?;
        match step {
            "turn_started" => {
                let turn_id = val.get("turn_id")?.as_str()?.to_string();
                let model = val
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("agy-unknown")
                    .to_string();
                Some(HarnessEvent::TurnStarted { turn_id, model })
            }
            "thought" => {
                let delta = val.get("text")?.as_str()?.to_string();
                Some(HarnessEvent::ThoughtDelta { delta })
            }
            "tool_call" => {
                let call_id = val.get("id")?.as_str()?.to_string();
                let tool_name = val.get("tool")?.as_str()?.to_string();
                let arguments = val.get("args").cloned().unwrap_or(serde_json::Value::Null);
                Some(HarnessEvent::ToolCallRequested {
                    call_id,
                    tool_name,
                    arguments,
                })
            }
            "tool_result" => {
                let call_id = val.get("id")?.as_str()?.to_string();
                let result = val
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let duration_ms = val.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                Some(HarnessEvent::ToolCallCompleted {
                    call_id,
                    result,
                    duration_ms,
                })
            }
            "turn_done" => {
                let stop_reason = val
                    .get("reason")
                    .and_then(|v| v.as_str())
                    .unwrap_or("stop")
                    .to_string();
                let prompt_tokens = val
                    .get("prompt_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let completion_tokens = val
                    .get("completion_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cached_tokens = val
                    .get("cached_tokens")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Some(HarnessEvent::TurnCompleted {
                    stop_reason,
                    metrics: TokenMetrics {
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens,
                    },
                })
            }
            _ => None,
        }
    }
}

/// OMP (Oh My Prompt) CLI streaming JSON line parser.
#[derive(Debug, Default)]
pub struct OmpStreamParser;

impl HarnessStreamParser for OmpStreamParser {
    fn parse_line(&mut self, line: &str) -> Option<HarnessEvent> {
        let val: serde_json::Value = serde_json::from_str(line).ok()?;
        let event = val.get("event")?.as_str()?;
        match event {
            "start" => {
                let turn_id = val.get("id")?.as_str()?.to_string();
                let model = val
                    .get("model")
                    .and_then(|v| v.as_str())
                    .unwrap_or("omp-unknown")
                    .to_string();
                Some(HarnessEvent::TurnStarted { turn_id, model })
            }
            "delta" => {
                let delta = val.get("content")?.as_str()?.to_string();
                Some(HarnessEvent::ThoughtDelta { delta })
            }
            "tool" => {
                let call_id = val.get("call_id")?.as_str()?.to_string();
                let tool_name = val.get("name")?.as_str()?.to_string();
                let arguments = val
                    .get("parameters")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                Some(HarnessEvent::ToolCallRequested {
                    call_id,
                    tool_name,
                    arguments,
                })
            }
            "tool_done" => {
                let call_id = val.get("call_id")?.as_str()?.to_string();
                let result = val
                    .get("result")
                    .cloned()
                    .unwrap_or(serde_json::Value::Null);
                let duration_ms = val.get("duration_ms").and_then(|v| v.as_u64()).unwrap_or(0);
                Some(HarnessEvent::ToolCallCompleted {
                    call_id,
                    result,
                    duration_ms,
                })
            }
            "finish" => {
                let stop_reason = val
                    .get("status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("completed")
                    .to_string();
                let usage = val.get("usage");
                let prompt_tokens = usage
                    .and_then(|u| u.get("prompt"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let completion_tokens = usage
                    .and_then(|u| u.get("completion"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let cached_tokens = usage
                    .and_then(|u| u.get("cached"))
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                Some(HarnessEvent::TurnCompleted {
                    stop_reason,
                    metrics: TokenMetrics {
                        prompt_tokens,
                        completion_tokens,
                        cached_tokens,
                    },
                })
            }
            _ => None,
        }
    }
}

/// An assembled harness process invocation (spec 0062).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HarnessInvocation {
    /// Target executable path or binary name.
    pub program: PathBuf,
    /// Ordered command-line arguments.
    pub args: Vec<String>,
    /// Sanitized environment variables.
    pub env: BTreeMap<String, String>,
    /// Working directory for the process.
    pub current_dir: PathBuf,
}

/// Allowed ambient environment variable keys that are safe to inherit (spec 0062).
pub const ALLOWED_AMBIENT_ENV: &[&str] = &[
    "PATH", "USER", "LOGNAME", "LANG", "LC_ALL", "LC_CTYPE", "TERM", "TZ", "SHELL",
];

/// Substrings that immediately disqualify an environment variable as a secret (spec 0062).
pub const SECRET_ENV_PATTERNS: &[&str] = &[
    "KEY",
    "TOKEN",
    "SECRET",
    "PASSWORD",
    "PASSWD",
    "CRED",
    "AUTH",
    "BEARER",
    "SIGNATURE",
];

/// Checks if an environment variable key matches a secret pattern.
#[must_use]
pub fn is_secret_env_key(key: &str) -> bool {
    let upper = key.to_ascii_uppercase();
    SECRET_ENV_PATTERNS.iter().any(|pat| upper.contains(pat))
}

/// Sanitized environment builder for agent harness processes (spec 0062).
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct HarnessEnv {
    vars: BTreeMap<String, String>,
}

impl HarnessEnv {
    /// Creates an empty sanitized environment.
    #[must_use]
    pub fn new() -> Self {
        Self {
            vars: BTreeMap::new(),
        }
    }

    /// Inherits safe ambient variables from the host environment, scrubbing secrets.
    pub fn inherit_safe_ambient(&mut self) -> &mut Self {
        for (k, v) in std::env::vars() {
            if ALLOWED_AMBIENT_ENV.contains(&k.as_str()) && !is_secret_env_key(&k) {
                self.vars.insert(k, v);
            }
        }
        self
    }

    /// Sets an explicit environment variable, refusing known secret patterns.
    pub fn set(
        &mut self,
        key: impl Into<String>,
        val: impl Into<String>,
    ) -> Result<&mut Self, EngineError> {
        let k = key.into();
        let v = val.into();
        if is_secret_env_key(&k) {
            return Err(EngineError::PolicyViolation(format!(
                "attempted to inject secret variable {k} into sanitized harness environment"
            )));
        }
        self.vars.insert(k, v);
        Ok(self)
    }

    /// Injects slot identity metadata into the environment.
    pub fn inject_slot_identity(
        &mut self,
        slot_id: &SlotId,
        lease_id: &LeaseId,
        session_id: &SessionId,
        provider: Provider,
        model: &str,
    ) -> &mut Self {
        self.vars
            .insert("UAS_SLOT_ID".into(), slot_id.as_str().into());
        self.vars
            .insert("UAS_LEASE_ID".into(), lease_id.as_str().into());
        self.vars
            .insert("UAS_SESSION_ID".into(), session_id.as_str().into());
        self.vars
            .insert("UAS_PROVIDER".into(), provider.as_str().into());
        self.vars.insert("UAS_MODEL".into(), model.into());
        self
    }

    /// Injects proxy settings pointing to the local proxy endpoint.
    pub fn inject_proxy(&mut self, proxy_url: &str) -> &mut Self {
        self.vars.insert("HTTP_PROXY".into(), proxy_url.into());
        self.vars.insert("HTTPS_PROXY".into(), proxy_url.into());
        self.vars.insert("ALL_PROXY".into(), proxy_url.into());
        self.vars.insert("http_proxy".into(), proxy_url.into());
        self.vars.insert("https_proxy".into(), proxy_url.into());
        self.vars.insert("all_proxy".into(), proxy_url.into());
        self
    }

    /// Isolates config directories to the slot scratch and configuration path.
    pub fn isolate_config_dirs(&mut self, slot_path: &Path) -> &mut Self {
        let config_dir = slot_path.join(".config");
        self.vars
            .insert("HOME".into(), slot_path.to_string_lossy().into());
        self.vars.insert(
            "XDG_CONFIG_HOME".into(),
            config_dir.to_string_lossy().into(),
        );
        self.vars.insert(
            "CLAUDE_CONFIG_DIR".into(),
            config_dir.join("claude").to_string_lossy().into(),
        );
        self.vars.insert(
            "AGY_CONFIG_DIR".into(),
            config_dir.join("agy").to_string_lossy().into(),
        );
        self
    }

    /// Returns the compiled environment map.
    #[must_use]
    pub fn into_map(self) -> BTreeMap<String, String> {
        self.vars
    }

    /// Returns a reference to the environment map.
    #[must_use]
    pub fn as_map(&self) -> &BTreeMap<String, String> {
        &self.vars
    }
}

/// Builder for constructing provider-specific CLI invocations (spec 0062).
#[derive(Debug, Clone)]
pub struct HarnessCommandBuilder {
    provider: Provider,
    slot_path: PathBuf,
    model: Option<String>,
    proxy_url: Option<String>,
    slot_id: Option<SlotId>,
    lease_id: Option<LeaseId>,
    session_id: Option<SessionId>,
    extra_env: BTreeMap<String, String>,
}

impl HarnessCommandBuilder {
    /// Creates a new builder for the designated provider and slot directory.
    pub fn new(provider: Provider, slot_path: impl Into<PathBuf>) -> Self {
        Self {
            provider,
            slot_path: slot_path.into(),
            model: None,
            proxy_url: None,
            slot_id: None,
            lease_id: None,
            session_id: None,
            extra_env: BTreeMap::new(),
        }
    }

    /// Sets the model override.
    pub fn with_model(&mut self, model: impl Into<String>) -> &mut Self {
        self.model = Some(model.into());
        self
    }

    /// Sets the socket proxy endpoint.
    pub fn with_proxy(&mut self, proxy_url: impl Into<String>) -> &mut Self {
        self.proxy_url = Some(proxy_url.into());
        self
    }

    /// Sets the slot identity parameters.
    pub fn with_slot_identity(
        &mut self,
        slot_id: SlotId,
        lease_id: LeaseId,
        session_id: SessionId,
    ) -> &mut Self {
        self.slot_id = Some(slot_id);
        self.lease_id = Some(lease_id);
        self.session_id = Some(session_id);
        self
    }

    /// Adds an extra safe environment variable.
    pub fn with_env(
        &mut self,
        key: impl Into<String>,
        val: impl Into<String>,
    ) -> Result<&mut Self, EngineError> {
        let k = key.into();
        let v = val.into();
        if is_secret_env_key(&k) {
            return Err(EngineError::PolicyViolation(format!(
                "prohibited secret variable {k} in extra_env"
            )));
        }
        self.extra_env.insert(k, v);
        Ok(self)
    }

    /// Validates the prompt and builds the complete execution invocation.
    pub fn build(&self, prompt: &str) -> Result<HarnessInvocation, EngineError> {
        if prompt.trim().is_empty() {
            return Err(EngineError::InvalidCommandInput {
                reason: "task prompt must not be empty or whitespace".into(),
            });
        }
        if prompt.contains('\0') {
            return Err(EngineError::InvalidCommandInput {
                reason: "task prompt contains forbidden null byte".into(),
            });
        }

        let model = self
            .model
            .as_deref()
            .unwrap_or_else(|| self.provider.default_model());

        let mut env = HarnessEnv::new();
        env.inherit_safe_ambient();
        env.isolate_config_dirs(&self.slot_path);

        if let (Some(slot_id), Some(lease_id), Some(session_id)) =
            (&self.slot_id, &self.lease_id, &self.session_id)
        {
            env.inject_slot_identity(slot_id, lease_id, session_id, self.provider, model);
        }

        if let Some(proxy) = &self.proxy_url {
            env.inject_proxy(proxy);
        }

        for (k, v) in &self.extra_env {
            env.set(k.clone(), v.clone())?;
        }

        let (program, args) = match self.provider {
            Provider::Claude => (
                PathBuf::from("claude"),
                vec![
                    "--print".into(),
                    "--output-format".into(),
                    "json".into(),
                    "-p".into(),
                    prompt.into(),
                    "--dangerously-skip-permissions".into(),
                ],
            ),
            Provider::Agy => (
                PathBuf::from("agy"),
                vec![
                    "run".into(),
                    "--stream".into(),
                    "--format".into(),
                    "jsonl".into(),
                    "--prompt".into(),
                    prompt.into(),
                ],
            ),
            Provider::Omp => (
                PathBuf::from("omp"),
                vec![
                    "exec".into(),
                    "--json".into(),
                    "--prompt".into(),
                    prompt.into(),
                ],
            ),
            Provider::Codex => (
                PathBuf::from("codex"),
                vec!["exec".into(), "--json".into(), prompt.into()],
            ),
            Provider::Gateway => (
                PathBuf::from("uas-gateway-client"),
                vec![
                    "--model".into(),
                    model.into(),
                    "--prompt".into(),
                    prompt.into(),
                ],
            ),
            Provider::Mock => (
                PathBuf::from("mock-harness"),
                vec!["--task".into(), prompt.into()],
            ),
        };

        Ok(HarnessInvocation {
            program,
            args,
            env: env.into_map(),
            current_dir: self.slot_path.clone(),
        })
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

    #[test]
    fn claude_stream_parser_translates_claude_json_lines() {
        let mut parser = ClaudeStreamParser;
        let start =
            r#"{"type":"message_start","message":{"id":"msg-123","model":"claude-3-7-sonnet"}}"#;
        assert_eq!(
            parser.parse_line(start),
            Some(HarnessEvent::TurnStarted {
                turn_id: "msg-123".into(),
                model: "claude-3-7-sonnet".into(),
            })
        );

        let delta = r#"{"type":"content_block_delta","delta":{"type":"text_delta","text":"Thinking step."}}"#;
        assert_eq!(
            parser.parse_line(delta),
            Some(HarnessEvent::ThoughtDelta {
                delta: "Thinking step.".into(),
            })
        );

        let tool = r#"{"type":"tool_use","id":"call-1","name":"bash","input":{"command":"ls"}}"#;
        assert_eq!(
            parser.parse_line(tool),
            Some(HarnessEvent::ToolCallRequested {
                call_id: "call-1".into(),
                tool_name: "bash".into(),
                arguments: serde_json::json!({"command": "ls"}),
            })
        );

        let done = r#"{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"output_tokens":25,"input_tokens":100}}"#;
        assert_eq!(
            parser.parse_line(done),
            Some(HarnessEvent::TurnCompleted {
                stop_reason: "end_turn".into(),
                metrics: TokenMetrics {
                    prompt_tokens: 100,
                    completion_tokens: 25,
                    cached_tokens: 0,
                },
            })
        );
    }

    #[test]
    fn agy_stream_parser_translates_agy_json_lines() {
        let mut parser = AgyStreamParser;
        let start = r#"{"step":"turn_started","turn_id":"agy-1","model":"agy-core"}"#;
        assert_eq!(
            parser.parse_line(start),
            Some(HarnessEvent::TurnStarted {
                turn_id: "agy-1".into(),
                model: "agy-core".into(),
            })
        );

        let thought = r#"{"step":"thought","text":"Exploring files"}"#;
        assert_eq!(
            parser.parse_line(thought),
            Some(HarnessEvent::ThoughtDelta {
                delta: "Exploring files".into(),
            })
        );

        let done = r#"{"step":"turn_done","reason":"stop","prompt_tokens":50,"completion_tokens":20,"cached_tokens":10}"#;
        assert_eq!(
            parser.parse_line(done),
            Some(HarnessEvent::TurnCompleted {
                stop_reason: "stop".into(),
                metrics: TokenMetrics {
                    prompt_tokens: 50,
                    completion_tokens: 20,
                    cached_tokens: 10,
                },
            })
        );
    }

    #[test]
    fn omp_stream_parser_translates_omp_json_lines() {
        let mut parser = OmpStreamParser;
        let start = r#"{"event":"start","id":"omp-1","model":"omp-fast"}"#;
        assert_eq!(
            parser.parse_line(start),
            Some(HarnessEvent::TurnStarted {
                turn_id: "omp-1".into(),
                model: "omp-fast".into(),
            })
        );

        let delta = r#"{"event":"delta","content":"Processing prompt"}"#;
        assert_eq!(
            parser.parse_line(delta),
            Some(HarnessEvent::ThoughtDelta {
                delta: "Processing prompt".into(),
            })
        );

        let finish = r#"{"event":"finish","status":"completed","usage":{"prompt":80,"completion":30,"cached":5}}"#;
        assert_eq!(
            parser.parse_line(finish),
            Some(HarnessEvent::TurnCompleted {
                stop_reason: "completed".into(),
                metrics: TokenMetrics {
                    prompt_tokens: 80,
                    completion_tokens: 30,
                    cached_tokens: 5,
                },
            })
        );
    }

    #[test]
    fn workflow_session_records_and_replays_harness_events() {
        use crate::{SessionId, WorkflowSession};
        use uas_domain::{LeaseId, SlotId};

        let session_id = SessionId::new("sess-harness").unwrap();
        let mut session = WorkflowSession::new(session_id.clone());

        session
            .start(
                SlotId::new("slot-1").unwrap(),
                LeaseId::new("lease-1").unwrap(),
                100,
            )
            .unwrap();

        let event = HarnessEvent::ThoughtDelta {
            delta: "Session thinking delta".into(),
        };
        session.handle_harness_event(event.clone(), 105).unwrap();

        assert_eq!(session.events().len(), 2);

        let replayed = WorkflowSession::replay_from_events(session_id, session.events()).unwrap();
        assert_eq!(replayed.events().len(), 2);
    }

    #[test]
    fn harness_command_builder_constructs_canonical_argv() {
        let slot_dir = PathBuf::from("/tmp/uas-slot-test");
        let builder = HarnessCommandBuilder::new(Provider::Claude, &slot_dir);
        let inv = builder.build("run tests").unwrap();
        assert_eq!(inv.program, PathBuf::from("claude"));
        assert_eq!(
            inv.args,
            vec![
                "--print",
                "--output-format",
                "json",
                "-p",
                "run tests",
                "--dangerously-skip-permissions"
            ]
        );
        assert_eq!(inv.current_dir, slot_dir);

        let agy_builder = HarnessCommandBuilder::new(Provider::Agy, &slot_dir);
        let agy_inv = agy_builder.build("fix bugs").unwrap();
        assert_eq!(agy_inv.program, PathBuf::from("agy"));
        assert_eq!(
            agy_inv.args,
            vec!["run", "--stream", "--format", "jsonl", "--prompt", "fix bugs"]
        );

        let omp_builder = HarnessCommandBuilder::new(Provider::Omp, &slot_dir);
        let omp_inv = omp_builder.build("refactor module").unwrap();
        assert_eq!(omp_inv.program, PathBuf::from("omp"));
        assert_eq!(
            omp_inv.args,
            vec!["exec", "--json", "--prompt", "refactor module"]
        );

        let mock_builder = HarnessCommandBuilder::new(Provider::Mock, &slot_dir);
        let mock_inv = mock_builder.build("mock task").unwrap();
        assert_eq!(mock_inv.program, PathBuf::from("mock-harness"));
        assert_eq!(mock_inv.args, vec!["--task", "mock task"]);
    }

    #[test]
    fn harness_command_builder_validates_prompt() {
        let slot_dir = PathBuf::from("/tmp/uas-slot-test");
        let builder = HarnessCommandBuilder::new(Provider::Mock, &slot_dir);

        let empty_res = builder.build("   ");
        assert!(matches!(
            empty_res,
            Err(EngineError::InvalidCommandInput { .. })
        ));

        let null_res = builder.build("invalid\0prompt");
        assert!(matches!(
            null_res,
            Err(EngineError::InvalidCommandInput { .. })
        ));
    }

    #[test]
    fn harness_env_scrubs_secret_patterns() {
        assert!(is_secret_env_key("GITHUB_TOKEN"));
        assert!(is_secret_env_key("AWS_SECRET_ACCESS_KEY"));
        assert!(is_secret_env_key("DB_PASSWORD"));
        assert!(is_secret_env_key("API_CREDENTIALS"));
        assert!(!is_secret_env_key("PATH"));
        assert!(!is_secret_env_key("USER"));
        assert!(!is_secret_env_key("LANG"));

        let mut env = HarnessEnv::new();
        let res = env.set("SUPER_SECRET_TOKEN", "12345");
        assert!(matches!(res, Err(EngineError::PolicyViolation(_))));
    }

    #[test]
    fn harness_env_injects_metadata_and_isolates_paths() {
        let slot_dir = PathBuf::from("/tmp/slots/slot-10");
        let mut env = HarnessEnv::new();
        env.isolate_config_dirs(&slot_dir);
        env.inject_slot_identity(
            &SlotId::new("slot-10").unwrap(),
            &LeaseId::new("lease-42").unwrap(),
            &SessionId::new("sess-99").unwrap(),
            Provider::Claude,
            "sonnet",
        );
        env.inject_proxy("http://127.0.0.1:8080");

        let map = env.into_map();
        assert_eq!(map.get("HOME").unwrap(), "/tmp/slots/slot-10");
        assert_eq!(
            map.get("XDG_CONFIG_HOME").unwrap(),
            "/tmp/slots/slot-10/.config"
        );
        assert_eq!(map.get("UAS_SLOT_ID").unwrap(), "slot-10");
        assert_eq!(map.get("UAS_LEASE_ID").unwrap(), "lease-42");
        assert_eq!(map.get("UAS_SESSION_ID").unwrap(), "sess-99");
        assert_eq!(map.get("UAS_PROVIDER").unwrap(), "claude");
        assert_eq!(map.get("UAS_MODEL").unwrap(), "sonnet");
        assert_eq!(map.get("HTTP_PROXY").unwrap(), "http://127.0.0.1:8080");
    }
}
