// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Pure types for the ceres seat inventory (spec 0058). No I/O.
//!
//! A value that could not be read is `None`, never zero (REQ-0058-05, D6).

use serde::{Deserialize, Serialize};

/// The `schema` field of the seat inventory document (REQ-0058-08).
pub const SEATS_SCHEMA: &str = "uas.seats.v1";

/// One cgroup limit: the literal `max`, or a number.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(try_from = "LimitRepr", into = "LimitRepr")]
pub enum Limit {
    /// No limit (`max`).
    Max,
    /// A limit of this many bytes, tasks or microseconds.
    Value(u64),
}

/// Wire form of [`Limit`]: the string `max` or a number.
#[derive(Serialize, Deserialize)]
#[serde(untagged)]
enum LimitRepr {
    Number(u64),
    Text(String),
}

impl From<Limit> for LimitRepr {
    fn from(limit: Limit) -> Self {
        match limit {
            Limit::Max => Self::Text("max".to_string()),
            Limit::Value(n) => Self::Number(n),
        }
    }
}

impl TryFrom<LimitRepr> for Limit {
    type Error = String;

    fn try_from(repr: LimitRepr) -> Result<Self, Self::Error> {
        match repr {
            LimitRepr::Number(n) => Ok(Self::Value(n)),
            LimitRepr::Text(t) if t == "max" => Ok(Self::Max),
            LimitRepr::Text(t) => Err(format!("limit must be a number or \"max\", got {t:?}")),
        }
    }
}

impl Limit {
    /// Parse the text of a cgroup limit file: `max` or a decimal number.
    pub fn parse(text: &str) -> Option<Self> {
        match text.trim() {
            "max" => Some(Self::Max),
            n => n.parse().ok().map(Self::Value),
        }
    }
}

/// The `cpu.max` pair: a quota and a period, both in microseconds.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct CpuMax {
    /// Quota per period, or no limit.
    pub quota_us: Limit,
    /// Period length.
    pub period_us: u64,
}

impl CpuMax {
    /// Parse the text of `cpu.max`, for example `max 100000` or `200000 100000`.
    pub fn parse(text: &str) -> Option<Self> {
        let mut parts = text.split_whitespace();
        let quota_us = Limit::parse(parts.next()?)?;
        let period_us = parts.next()?.parse().ok()?;
        parts.next().is_none().then_some(CpuMax {
            quota_us,
            period_us,
        })
    }
}

/// Whether a scope is a tmux pane.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    /// A `tmux-spawn-<uuid>.scope`.
    TmuxPane,
    /// Any other scope.
    Other,
}

impl ScopeKind {
    /// Classify a scope by its directory name.
    pub fn of_scope_name(name: &str) -> Self {
        if name.starts_with("tmux-spawn-") && name.ends_with(".scope") {
            Self::TmuxPane
        } else {
            Self::Other
        }
    }
}

/// One scope under the user service cgroup (REQ-0058-01).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatSample {
    /// Directory name, for example `tmux-spawn-<uuid>.scope`.
    pub scope: String,
    /// Whether the scope is a tmux pane.
    pub kind: ScopeKind,
    /// The tmux session name of a pane in this scope; `None` when unattributed (REQ-0058-03/04).
    pub session: Option<String>,
    /// `memory.current`, bytes.
    pub memory_current: Option<u64>,
    /// `memory.peak`, bytes.
    pub memory_peak: Option<u64>,
    /// `memory.high`.
    pub memory_high: Option<Limit>,
    /// `memory.max`.
    pub memory_max: Option<Limit>,
    /// `pids.current`.
    pub pids_current: Option<u64>,
    /// `pids.max`.
    pub pids_max: Option<Limit>,
    /// `cpu.max`.
    pub cpu_max: Option<CpuMax>,
    /// `cpu.weight`.
    pub cpu_weight: Option<u64>,
    /// `some avg10` of `memory.pressure`, percent.
    pub memory_pressure_some_avg10: Option<f64>,
    /// `some avg10` of `cpu.pressure`, percent.
    pub cpu_pressure_some_avg10: Option<f64>,
}

impl SeatSample {
    /// A sample for `scope` with every reading absent and no seat attributed.
    pub fn empty(scope: &str) -> Self {
        Self {
            scope: scope.to_string(),
            kind: ScopeKind::of_scope_name(scope),
            session: None,
            memory_current: None,
            memory_peak: None,
            memory_high: None,
            memory_max: None,
            pids_current: None,
            pids_max: None,
            cpu_max: None,
            cpu_weight: None,
            memory_pressure_some_avg10: None,
            cpu_pressure_some_avg10: None,
        }
    }
}

/// The host headroom gate (D4, REQ-0058-07): load below a ceiling and memory above a floor.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct HeadroomGate {
    /// 1-minute load average; `None` if unreadable.
    pub load_1m: Option<f64>,
    /// `MemAvailable`, bytes; `None` if unreadable.
    pub mem_available_bytes: Option<u64>,
    /// The gate needs a 1-minute load strictly below this.
    pub max_load_1m: f64,
    /// The gate needs at least this many bytes available.
    pub min_mem_available_bytes: u64,
    /// True only when both readings exist and both thresholds hold.
    pub passes: bool,
}

impl HeadroomGate {
    /// Default load ceiling (D4).
    pub const DEFAULT_MAX_LOAD_1M: f64 = 12.0;
    /// Default memory floor, 16 GiB (D4).
    pub const DEFAULT_MIN_MEM_AVAILABLE_BYTES: u64 = 16 * 1024 * 1024 * 1024;

    /// Evaluate the gate. An unreadable value fails it: headroom cannot be shown.
    pub fn evaluate(
        load_1m: Option<f64>,
        mem_available_bytes: Option<u64>,
        max_load_1m: f64,
        min_mem_available_bytes: u64,
    ) -> Self {
        let passes = matches!((load_1m, mem_available_bytes), (Some(l), Some(m))
            if l < max_load_1m && m >= min_mem_available_bytes);
        Self {
            load_1m,
            mem_available_bytes,
            max_load_1m,
            min_mem_available_bytes,
            passes,
        }
    }
}

/// Whether tmux could be asked for panes (REQ-0058-09).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TmuxStatus {
    /// True when `list-panes` ran and succeeded.
    pub available: bool,
    /// Why not, when `available` is false.
    pub reason: Option<String>,
}

/// The `uas.seats.v1` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatsDocument {
    /// Always [`SEATS_SCHEMA`].
    pub schema: String,
    /// The headroom gate.
    pub headroom: HeadroomGate,
    /// The tmux attribution status.
    pub tmux: TmuxStatus,
    /// Every direct child scope, attributed or not.
    pub scopes: Vec<SeatSample>,
}

impl SeatsDocument {
    /// Build a document with the schema field set.
    pub fn new(headroom: HeadroomGate, tmux: TmuxStatus, scopes: Vec<SeatSample>) -> Self {
        Self {
            schema: SEATS_SCHEMA.to_string(),
            headroom,
            tmux,
            scopes,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const GIB: u64 = 1024 * 1024 * 1024;

    #[test]
    fn seat_gate_fails_at_load_12_and_at_15_9_gib() {
        let at_load = HeadroomGate::evaluate(Some(12.0), Some(20 * GIB), 12.0, 16 * GIB);
        assert!(!at_load.passes, "load 12.0 must fail a gate of < 12");
        let low_mem = HeadroomGate::evaluate(Some(1.0), Some(GIB * 159 / 10), 12.0, 16 * GIB);
        assert!(!low_mem.passes, "15.9 GiB must fail a floor of 16 GiB");
        let ok = HeadroomGate::evaluate(Some(11.9), Some(16 * GIB), 12.0, 16 * GIB);
        assert!(ok.passes);
    }

    #[test]
    fn seat_gate_fails_when_a_reading_is_absent() {
        assert!(!HeadroomGate::evaluate(None, Some(20 * GIB), 12.0, 16 * GIB).passes);
        assert!(!HeadroomGate::evaluate(Some(1.0), None, 12.0, 16 * GIB).passes);
    }

    #[test]
    fn seat_document_round_trips_and_carries_schema() {
        let mut sample = SeatSample::empty("tmux-spawn-abc.scope");
        sample.memory_max = Some(Limit::Max);
        sample.pids_max = Some(Limit::Value(67473));
        sample.cpu_max = CpuMax::parse("max 100000");
        let doc = SeatsDocument::new(
            HeadroomGate::evaluate(Some(1.0), Some(20 * GIB), 12.0, 16 * GIB),
            TmuxStatus {
                available: false,
                reason: Some("no server".into()),
            },
            vec![sample],
        );
        let text = serde_json::to_string(&doc).unwrap();
        let value: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(value["schema"], "uas.seats.v1");
        assert_eq!(value["scopes"][0]["memory_max"], "max");
        assert_eq!(value["scopes"][0]["pids_max"], 67473);
        assert!(value["scopes"][0]["memory_peak"].is_null());
        let back: SeatsDocument = serde_json::from_str(&text).unwrap();
        assert_eq!(back, doc);
    }

    #[test]
    fn seat_limit_rejects_other_text() {
        assert!(serde_json::from_str::<Limit>("\"lots\"").is_err());
        assert_eq!(Limit::parse("max\n"), Some(Limit::Max));
        assert_eq!(Limit::parse(""), None);
        assert_eq!(
            CpuMax::parse("200000 100000").unwrap().quota_us,
            Limit::Value(200_000)
        );
        assert_eq!(CpuMax::parse("max"), None);
    }

    #[test]
    fn seat_scope_kind_by_name() {
        assert_eq!(
            ScopeKind::of_scope_name("tmux-spawn-1.scope"),
            ScopeKind::TmuxPane
        );
        assert_eq!(ScopeKind::of_scope_name("app-x.scope"), ScopeKind::Other);
    }
}
