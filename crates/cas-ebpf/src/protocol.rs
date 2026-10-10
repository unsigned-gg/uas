//! The `cas-ebpfd` line protocol (spec 0014 D8, ruling R3).
//!
//! One JSON object per line over a Unix socket. The client sends a
//! [`Request`] and the daemon answers with a [`Response`]. After a `stream`
//! request the daemon keeps sending [`StreamLine`] frames until the client
//! closes. Any request that is not on the allowlist gets an error frame and
//! the connection stays open. A line over [`MAX_LINE`] bytes closes it.

use serde::{Deserialize, Serialize};

use crate::{error::EbpfError, wire::ExecRecord};

/// Longest accepted line in bytes, not counting the newline.
pub const MAX_LINE: usize = 4096;

/// A request frame. The `op` field selects the variant.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "op", rename_all = "lowercase")]
pub enum Request {
    /// Start tracking `pid`, recording `ppid` as its parent.
    Track {
        /// Process id to track.
        pid: u32,
        /// Its parent process id.
        ppid: u32,
    },
    /// Stop tracking `pid`.
    Untrack {
        /// Process id to forget.
        pid: u32,
    },
    /// Ask for the tracked count and the dropped counter.
    Status,
    /// Switch the connection to an event stream.
    Stream,
    /// Read the receipt window opened by a `track` of `pid` (spec 0014 D14).
    Receipt {
        /// The root pid whose window to read.
        pid: u32,
    },
    /// Discard the receipt window of `pid` (spec 0014 D14).
    Forget {
        /// The root pid whose window to discard.
        pid: u32,
    },
}

/// The totals of one root's receipt window (spec 0014 D14).
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct ReceiptFrame {
    /// Hex SHA-256 of the window's transcript, in delivery order.
    pub run_hash: String,
    /// Records absorbed into the window.
    pub events: u64,
    /// Records that may be missing from `run_hash` (spec 0014 D14).
    pub dropped_events: u64,
    /// Timestamp of the first absorbed record, 0 when there is none.
    pub first_ns: u64,
    /// Timestamp of the last absorbed record, 0 when there is none.
    pub last_ns: u64,
}

/// A response frame to one request.
///
/// On the wire the variants are told apart by their fields, so the enum is
/// untagged. The order matters when reading: the most specific shape is first.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Response {
    /// `{"receipt":{...}}`: the totals of one root's window (spec 0014 D14).
    Receipt {
        /// The window totals.
        receipt: ReceiptFrame,
    },
    /// `{"ok":true,"tracked":K,"dropped_events":D}`.
    Status {
        /// Always `true`.
        ok: bool,
        /// Pids the daemon tracks that the kernel map still holds.
        tracked: u64,
        /// Records the kernel could not reserve, summed over all cpus.
        dropped_events: u64,
    },
    /// `{"ok":false,"error":"..."}`.
    Failure {
        /// Always `false`.
        ok: bool,
        /// A short reason.
        error: String,
    },
    /// `{"ok":true}`.
    Ack {
        /// Always `true`.
        ok: bool,
    },
}

impl Response {
    /// The plain success frame.
    pub fn ack() -> Self {
        Self::Ack { ok: true }
    }

    /// The error frame.
    pub fn failure(error: impl Into<String>) -> Self {
        Self::Failure {
            ok: false,
            error: error.into(),
        }
    }

    /// The status frame.
    pub fn status(tracked: u64, dropped_events: u64) -> Self {
        Self::Status {
            ok: true,
            tracked,
            dropped_events,
        }
    }
}

/// A line the daemon sends on a stream connection.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum StreamLine {
    /// `{"event":{...}}`: one exec of a tracked pid.
    Event(ExecRecord),
    /// `{"dropped":N}`: the kernel dropped records. `N` is the running total
    /// of the kernel counter.
    Dropped(u64),
    /// `{"lagged":N}`: this client fell behind and missed `N` event lines.
    Lagged(u64),
}

/// Parses one request line.
///
/// Bad JSON, an unknown `op` and missing fields all give [`EbpfError::Protocol`].
pub fn parse_request(line: &str) -> Result<Request, EbpfError> {
    serde_json::from_str(line.trim_end_matches(['\r', '\n']))
        .map_err(|e| EbpfError::Protocol(format!("bad request: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::wire::ExecEvent;

    #[test]
    fn requests_have_the_contract_shape() {
        let cases = [
            (
                Request::Track { pid: 7, ppid: 3 },
                r#"{"op":"track","pid":7,"ppid":3}"#,
            ),
            (Request::Untrack { pid: 7 }, r#"{"op":"untrack","pid":7}"#),
            (Request::Status, r#"{"op":"status"}"#),
            (Request::Stream, r#"{"op":"stream"}"#),
            (Request::Receipt { pid: 7 }, r#"{"op":"receipt","pid":7}"#),
            (Request::Forget { pid: 7 }, r#"{"op":"forget","pid":7}"#),
        ];
        for (request, text) in cases {
            assert_eq!(serde_json::to_string(&request).expect("ser"), text);
            assert_eq!(parse_request(text).expect("parse"), request);
        }
    }

    #[test]
    fn parse_accepts_a_trailing_newline() {
        assert_eq!(
            parse_request("{\"op\":\"status\"}\r\n").expect("parse"),
            Request::Status
        );
    }

    #[test]
    fn allowlist_rejects_everything_else() {
        for line in [
            r#"{"op":"shell","cmd":"id"}"#,
            r#"{"op":"Track","pid":1,"ppid":1}"#,
            r#"{"op":"track","pid":1}"#,
            r#"{"op":"track","pid":-1,"ppid":1}"#,
            r#"{"pid":1}"#,
            r#"["track"]"#,
            r#"{"op":"receipt"}"#,
            "not json",
            "",
        ] {
            assert!(
                matches!(parse_request(line), Err(EbpfError::Protocol(_))),
                "accepted {line:?}"
            );
        }
    }

    #[test]
    fn responses_have_the_contract_shape() {
        let cases = [
            (Response::ack(), r#"{"ok":true}"#),
            (
                Response::status(2, 5),
                r#"{"ok":true,"tracked":2,"dropped_events":5}"#,
            ),
            (Response::failure("nope"), r#"{"ok":false,"error":"nope"}"#),
            (
                Response::Receipt {
                    receipt: ReceiptFrame {
                        run_hash: "ab".into(),
                        events: 2,
                        dropped_events: 1,
                        first_ns: 3,
                        last_ns: 4,
                    },
                },
                r#"{"receipt":{"run_hash":"ab","events":2,"dropped_events":1,"first_ns":3,"last_ns":4}}"#,
            ),
        ];
        for (response, text) in cases {
            assert_eq!(serde_json::to_string(&response).expect("ser"), text);
            assert_eq!(
                serde_json::from_str::<Response>(text).expect("de"),
                response
            );
        }
    }

    #[test]
    fn stream_lines_have_the_contract_shape() {
        let record = ExecEvent::new(1, 2, 3, 4, "cc", "/usr/bin/cc").record();
        let event = StreamLine::Event(record);
        let text = serde_json::to_string(&event).expect("ser");
        assert_eq!(
            text,
            r#"{"event":{"timestamp_ns":1,"pid":2,"ns_pid":2,"ppid":3,"uid":4,"comm":"cc","filename":"/usr/bin/cc"}}"#
        );
        assert_eq!(
            serde_json::from_str::<StreamLine>(&text).expect("de"),
            event
        );

        let dropped = StreamLine::Dropped(9);
        let text = serde_json::to_string(&dropped).expect("ser");
        assert_eq!(text, r#"{"dropped":9}"#);
        assert_eq!(
            serde_json::from_str::<StreamLine>(&text).expect("de"),
            dropped
        );

        let lagged = StreamLine::Lagged(4);
        let text = serde_json::to_string(&lagged).expect("ser");
        assert_eq!(text, r#"{"lagged":4}"#);
        assert_eq!(
            serde_json::from_str::<StreamLine>(&text).expect("de"),
            lagged
        );
    }
}
