// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Pure types for the dry-run seat limit plan (spec 0058 wave 2). No I/O.
//!
//! The policy file is read and parsed by the caller into [`RawPolicy`]. [`SeatLimitPolicy`]
//! is built from it once, so every later step works on validated values.

use std::fmt;

use serde::{Deserialize, Serialize};

use crate::seat::{Attribution, HeadroomGate, SeatsDocument};

/// The `schema` field of the plan document.
pub const SEATS_PLAN_SCHEMA: &str = "uas.seats.plan.v1";

/// The statement every plan carries (REQ-0058-18).
pub const NO_APPLY_NOTICE: &str =
    "uas applied nothing. Enforcement belongs to seat launch, not to uas.";

/// Why a policy was rejected.
#[derive(Debug, Clone, PartialEq, thiserror::Error)]
pub enum PolicyError {
    /// A size is not a number with an optional K, M, G or T suffix.
    #[error("{field}: {value:?} is not a size such as 4G or 512M")]
    BadSize {
        /// The policy key.
        field: &'static str,
        /// The text given.
        value: String,
    },
    /// A size spells "no limit" (`infinity`, `max`); the policy omits the field instead.
    #[error("{field}: {value:?} is not a size; omit the field for no limit")]
    NoLimitSpelling {
        /// The policy key.
        field: &'static str,
        /// The text given.
        value: String,
    },
    /// A size, or `tasks_max`, is zero.
    #[error("{field}: must be greater than zero")]
    Zero {
        /// The policy key.
        field: &'static str,
    },
    /// `cpu_weight` is outside 1 to 10000.
    #[error("cpu_weight: {0} is outside 1 to 10000")]
    BadCpuWeight(u32),
    /// `memory_max` is set with no `memory_high` at or below it (REQ-0058-12).
    #[error("{block}: memory_max needs a memory_high at or below it")]
    MaxWithoutHigh {
        /// `default` or `seat <session>`.
        block: String,
    },
    /// Two `[[seat]]` entries name one session.
    #[error("seat {0:?} is listed twice")]
    DuplicateSeat(String),
    /// A `[[seat]]` has an empty session name.
    #[error("a [[seat]] has an empty session")]
    EmptySession,
    /// `max_load_1m` is not a positive finite number.
    #[error("max_load_1m: {0} is not a positive number")]
    BadLoad(f64),
}

/// A byte size, parsed from systemd syntax (`4G`, `512M`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Size(u64);

impl Size {
    /// Parse systemd size syntax: digits with an optional fraction (`1.5G`) and an optional
    /// `K`, `M`, `G` or `T` suffix in powers of 1024. The result is truncated to whole bytes.
    pub fn parse(field: &'static str, text: &str) -> Result<Self, PolicyError> {
        let bad = || PolicyError::BadSize {
            field,
            value: text.to_string(),
        };
        let t = text.trim();
        if t.eq_ignore_ascii_case("infinity") || t.eq_ignore_ascii_case("max") {
            return Err(PolicyError::NoLimitSpelling {
                field,
                value: text.to_string(),
            });
        }
        let (number, shift) = match t.chars().last().ok_or_else(bad)? {
            'K' => (&t[..t.len() - 1], 10u32),
            'M' => (&t[..t.len() - 1], 20),
            'G' => (&t[..t.len() - 1], 30),
            'T' => (&t[..t.len() - 1], 40),
            _ => (t, 0),
        };
        let (int, frac) = number.split_once('.').unwrap_or((number, ""));
        let digits = |s: &str| s.bytes().all(|b| b.is_ascii_digit());
        let has_dot = number.contains('.');
        if int.is_empty() || !digits(int) || !digits(frac) || (has_dot && frac.is_empty()) {
            return Err(bad());
        }
        // 18 fraction digits keep 10^18 * u64 * 2^40 inside u128 (checked anyway).
        if frac.len() > 18 {
            return Err(bad());
        }
        let scale = 10u128.pow(frac.len() as u32);
        let int_n: u128 = int.parse().map_err(|_| bad())?;
        let frac_n: u128 = if frac.is_empty() {
            0
        } else {
            frac.parse().map_err(|_| bad())?
        };
        let scaled = int_n
            .checked_mul(scale)
            .and_then(|v| v.checked_add(frac_n))
            .and_then(|v| v.checked_mul(1u128 << shift))
            .ok_or_else(bad)?;
        let bytes = u64::try_from(scaled / scale).map_err(|_| bad())?;
        if bytes == 0 {
            return Err(PolicyError::Zero { field });
        }
        Ok(Self(bytes))
    }

    /// The size in bytes.
    pub fn bytes(self) -> u64 {
        self.0
    }
}

impl fmt::Display for Size {
    /// The largest suffix that divides the size exactly, so `4G` round-trips.
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        for (suffix, shift) in [("T", 40), ("G", 30), ("M", 20), ("K", 10)] {
            if self.0.is_multiple_of(1 << shift) {
                return write!(f, "{}{suffix}", self.0 >> shift);
            }
        }
        write!(f, "{}", self.0)
    }
}

/// The limits one seat would get. An absent field is not proposed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatLimits {
    /// `MemoryHigh`.
    pub memory_high: Option<Size>,
    /// `MemoryMax`.
    pub memory_max: Option<Size>,
    /// `TasksMax`.
    pub tasks_max: Option<u64>,
    /// `CPUWeight`.
    pub cpu_weight: Option<u32>,
}

impl SeatLimits {
    /// These limits over `base`: each field set here wins, the rest come from `base`.
    pub fn over(self, base: SeatLimits) -> SeatLimits {
        SeatLimits {
            memory_high: self.memory_high.or(base.memory_high),
            memory_max: self.memory_max.or(base.memory_max),
            tasks_max: self.tasks_max.or(base.tasks_max),
            cpu_weight: self.cpu_weight.or(base.cpu_weight),
        }
    }

    /// REQ-0058-12: `memory_max` needs a `memory_high` at or below it.
    fn check(&self, block: &str) -> Result<(), PolicyError> {
        match (self.memory_high, self.memory_max) {
            (_, None) => Ok(()),
            (Some(high), Some(max)) if high <= max => Ok(()),
            _ => Err(PolicyError::MaxWithoutHigh {
                block: block.to_string(),
            }),
        }
    }
}

/// A `[[seat]]` entry: limits for the seat with this exact session name.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SeatOverride {
    /// The tmux session name.
    pub session: String,
    /// The fields this seat overrides.
    pub limits: SeatLimits,
}

/// The `[host]` table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize)]
pub struct HostPolicy {
    /// The most attributed seats the host should hold (REQ-0058-17).
    pub max_attributed_seats: Option<u64>,
    /// Overrides the headroom gate's load ceiling.
    pub max_load_1m: Option<f64>,
    /// Overrides the headroom gate's memory floor.
    pub min_mem_available: Option<Size>,
}

/// A validated policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatLimitPolicy {
    /// The `[default]` table.
    pub default: SeatLimits,
    /// The `[[seat]]` entries, in file order.
    pub seats: Vec<SeatOverride>,
    /// The `[host]` table.
    pub host: HostPolicy,
}

/// Raw limit fields, as the policy file spells them.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawLimits {
    /// `memory_high`, a size string.
    pub memory_high: Option<String>,
    /// `memory_max`, a size string.
    pub memory_max: Option<String>,
    /// `tasks_max`.
    pub tasks_max: Option<u64>,
    /// `cpu_weight`.
    pub cpu_weight: Option<u32>,
}

/// A raw `[[seat]]` entry.
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawSeat {
    /// The exact tmux session name.
    pub session: String,
    /// `memory_high`, a size string.
    pub memory_high: Option<String>,
    /// `memory_max`, a size string.
    pub memory_max: Option<String>,
    /// `tasks_max`.
    pub tasks_max: Option<u64>,
    /// `cpu_weight`.
    pub cpu_weight: Option<u32>,
}

/// A raw `[host]` table.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawHost {
    /// `max_attributed_seats`.
    pub max_attributed_seats: Option<u64>,
    /// `max_load_1m`.
    pub max_load_1m: Option<f64>,
    /// `min_mem_available`, a size string.
    pub min_mem_available: Option<String>,
}

/// The policy file as parsed, before validation.
#[derive(Debug, Clone, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct RawPolicy {
    /// `[default]`.
    #[serde(default)]
    pub default: RawLimits,
    /// `[[seat]]`.
    #[serde(default)]
    pub seat: Vec<RawSeat>,
    /// `[host]`.
    #[serde(default)]
    pub host: RawHost,
}

fn size(field: &'static str, v: &Option<String>) -> Result<Option<Size>, PolicyError> {
    v.as_deref().map(|t| Size::parse(field, t)).transpose()
}

fn limits(
    high: &Option<String>,
    max: &Option<String>,
    tasks: Option<u64>,
    weight: Option<u32>,
) -> Result<SeatLimits, PolicyError> {
    if tasks == Some(0) {
        return Err(PolicyError::Zero { field: "tasks_max" });
    }
    if let Some(w) = weight {
        if !(1..=10_000).contains(&w) {
            return Err(PolicyError::BadCpuWeight(w));
        }
    }
    Ok(SeatLimits {
        memory_high: size("memory_high", high)?,
        memory_max: size("memory_max", max)?,
        tasks_max: tasks,
        cpu_weight: weight,
    })
}

impl SeatLimitPolicy {
    /// Validate a raw policy: parse sizes, range-check, reject duplicates, and apply
    /// REQ-0058-12 to the default and to each seat merged over it.
    pub fn from_raw(raw: &RawPolicy) -> Result<Self, PolicyError> {
        let d = &raw.default;
        let default = limits(&d.memory_high, &d.memory_max, d.tasks_max, d.cpu_weight)?;
        default.check("default")?;
        let mut seats: Vec<SeatOverride> = Vec::new();
        for s in &raw.seat {
            if s.session.is_empty() {
                return Err(PolicyError::EmptySession);
            }
            if seats.iter().any(|o| o.session == s.session) {
                return Err(PolicyError::DuplicateSeat(s.session.clone()));
            }
            let own = limits(&s.memory_high, &s.memory_max, s.tasks_max, s.cpu_weight)?;
            own.over(default).check(&format!("seat {}", s.session))?;
            seats.push(SeatOverride {
                session: s.session.clone(),
                limits: own,
            });
        }
        let h = &raw.host;
        if let Some(l) = h.max_load_1m {
            if !(l.is_finite() && l > 0.0) {
                return Err(PolicyError::BadLoad(l));
            }
        }
        Ok(Self {
            default,
            seats,
            host: HostPolicy {
                max_attributed_seats: h.max_attributed_seats,
                max_load_1m: h.max_load_1m,
                min_mem_available: size("min_mem_available", &h.min_mem_available)?,
            },
        })
    }
}

/// One attributed scope and what a launcher would set on it (REQ-0058-11, REQ-0058-16).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PlannedSeat {
    /// The scope directory name.
    pub scope: String,
    /// Every pane in the scope.
    pub attributions: Vec<Attribution>,
    /// The `[[seat]]` session that matched, when one did.
    pub matched_session: Option<String>,
    /// `memory.current`, bytes.
    pub memory_current: Option<u64>,
    /// `memory.peak`, bytes.
    pub memory_peak: Option<u64>,
    /// The limits the policy proposes.
    pub proposed: SeatLimits,
    /// True when the peak exceeds the proposed `MemoryHigh`; `None` when either is absent.
    pub peak_over_high: Option<bool>,
}

/// The attributed count against `max_attributed_seats` (REQ-0058-17).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Ceiling {
    /// The policy ceiling.
    pub max_attributed_seats: u64,
    /// Scopes attributed to a pane now.
    pub attributed: usize,
    /// True when `attributed` is above the ceiling.
    pub over: bool,
}

/// The `uas.seats.plan.v1` document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SeatLimitPlan {
    /// Always [`SEATS_PLAN_SCHEMA`].
    pub schema: String,
    /// Always [`NO_APPLY_NOTICE`] (REQ-0058-18).
    pub notice: String,
    /// The headroom gate, with any policy overrides applied.
    pub headroom: HeadroomGate,
    /// The seat ceiling, when the policy sets one.
    pub ceiling: Option<Ceiling>,
    /// One entry per attributed scope.
    pub seats: Vec<PlannedSeat>,
    /// How many seats have a peak over the proposed `MemoryHigh`.
    pub marked_over_high: usize,
    /// `[[seat]]` sessions that no attributed scope holds.
    pub unknown_sessions: Vec<String>,
}

impl SeatLimitPlan {
    /// Plan limits for every attributed scope in `doc`. Pure: nothing is applied.
    pub fn build(policy: &SeatLimitPolicy, doc: &SeatsDocument) -> Self {
        let mut seats = Vec::new();
        let mut named: Vec<&str> = Vec::new();
        for s in doc.scopes.iter().filter(|s| !s.attributions.is_empty()) {
            named.extend(s.attributions.iter().map(|a| a.session.as_str()));
            // When several panes match different entries, the first in sorted session order
            // wins, whatever order the document lists them in.
            let mut sessions: Vec<&str> =
                s.attributions.iter().map(|a| a.session.as_str()).collect();
            sessions.sort_unstable();
            let matched = sessions
                .iter()
                .find_map(|name| policy.seats.iter().find(|o| o.session == *name));
            let proposed = matched.map_or(policy.default, |o| o.limits.over(policy.default));
            let peak_over_high = match (s.memory_peak, proposed.memory_high) {
                (Some(peak), Some(high)) => Some(peak > high.bytes()),
                _ => None,
            };
            seats.push(PlannedSeat {
                scope: s.scope.clone(),
                attributions: s.attributions.clone(),
                matched_session: matched.map(|o| o.session.clone()),
                memory_current: s.memory_current,
                memory_peak: s.memory_peak,
                proposed,
                peak_over_high,
            });
        }
        let attributed = seats.len();
        Self {
            schema: SEATS_PLAN_SCHEMA.to_string(),
            notice: NO_APPLY_NOTICE.to_string(),
            headroom: doc.headroom.clone(),
            ceiling: policy.host.max_attributed_seats.map(|max| Ceiling {
                max_attributed_seats: max,
                attributed,
                over: attributed as u64 > max,
            }),
            marked_over_high: seats
                .iter()
                .filter(|p| p.peak_over_high == Some(true))
                .count(),
            unknown_sessions: policy
                .seats
                .iter()
                .filter(|o| !named.contains(&o.session.as_str()))
                .map(|o| o.session.clone())
                .collect(),
            seats,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::seat::{SeatSample, TmuxStatus};
    use crate::SocketCounts;

    const G: u64 = 1 << 30;

    fn raw(high: Option<&str>, max: Option<&str>) -> RawPolicy {
        RawPolicy {
            default: RawLimits {
                memory_high: high.map(str::to_string),
                memory_max: max.map(str::to_string),
                ..RawLimits::default()
            },
            ..RawPolicy::default()
        }
    }

    fn seat_raw(session: &str, high: Option<&str>, max: Option<&str>) -> RawSeat {
        RawSeat {
            session: session.into(),
            memory_high: high.map(str::to_string),
            memory_max: max.map(str::to_string),
            tasks_max: None,
            cpu_weight: None,
        }
    }

    fn doc(scopes: Vec<SeatSample>) -> SeatsDocument {
        SeatsDocument::new(
            HeadroomGate::evaluate(Some(1.0), Some(20 * G), 12.0, 16 * G),
            TmuxStatus {
                available: true,
                reason: None,
                sockets: vec![],
                counts: SocketCounts::default(),
                over_cap_reason: None,
            },
            scopes,
        )
    }

    fn attributed(scope: &str, session: &str, peak: Option<u64>) -> SeatSample {
        let mut s = SeatSample::empty(scope);
        s.attributions = vec![Attribution {
            session: session.into(),
            socket: None,
        }];
        s.memory_peak = peak;
        s.memory_current = Some(G);
        s
    }

    #[test]
    fn seat_size_parses_systemd_syntax_and_rejects_garbage() {
        assert_eq!(Size::parse("f", "4G").unwrap().bytes(), 4 * G);
        assert_eq!(Size::parse("f", "512M").unwrap().bytes(), 512 << 20);
        assert_eq!(Size::parse("f", "1024").unwrap().bytes(), 1024);
        assert_eq!(Size::parse("f", "4G").unwrap().to_string(), "4G");
        for bad in [
            "",
            "G",
            "4g",
            "4GB",
            "-1",
            "-1G",
            "four",
            "4 G",
            "99999999999999T",
            "1.2.3G",
            "1.",
            ".5G",
            "1.G",
            "99999999999999999999",
        ] {
            assert!(
                matches!(Size::parse("f", bad), Err(PolicyError::BadSize { .. })),
                "{bad:?} must be rejected"
            );
        }
        for zero in ["0", "0G", "0.0G"] {
            assert!(
                matches!(Size::parse("f", zero), Err(PolicyError::Zero { .. })),
                "{zero:?}"
            );
        }
    }

    #[test]
    fn seat_size_accepts_decimals_like_systemd_and_truncates() {
        assert_eq!(Size::parse("f", "1.5G").unwrap().bytes(), 1_610_612_736);
        assert_eq!(Size::parse("f", "0.5M").unwrap().bytes(), 524_288);
        assert_eq!(
            Size::parse("f", "1.0001K").unwrap().bytes(),
            1024,
            "truncated"
        );
        assert_eq!(Size::parse("f", "1.5G").unwrap().to_string(), "1536M");
    }

    #[test]
    fn seat_size_tells_the_user_to_omit_the_field_for_no_limit() {
        for word in ["infinity", "max", "Infinity", "MAX"] {
            let e = Size::parse("memory_max", word).unwrap_err();
            assert!(matches!(e, PolicyError::NoLimitSpelling { .. }), "{word}");
            assert!(e.to_string().contains("omit the field for no limit"));
        }
    }

    #[test]
    fn seat_policy_rejects_max_without_high_at_or_below_it() {
        assert!(SeatLimitPolicy::from_raw(&raw(None, Some("4G"))).is_err());
        assert!(SeatLimitPolicy::from_raw(&raw(Some("8G"), Some("4G"))).is_err());
        assert!(SeatLimitPolicy::from_raw(&raw(Some("4G"), Some("4G"))).is_ok());
        assert!(SeatLimitPolicy::from_raw(&raw(Some("2G"), Some("4G"))).is_ok());
        // A seat with a max and no high of its own, over a default with no high.
        let mut p = raw(None, None);
        p.seat.push(seat_raw("a", None, Some("4G")));
        assert!(matches!(
            SeatLimitPolicy::from_raw(&p),
            Err(PolicyError::MaxWithoutHigh { .. })
        ));
        // The same seat is fine when the default supplies a high below it.
        let mut p = raw(Some("2G"), None);
        p.seat.push(seat_raw("a", None, Some("4G")));
        assert!(SeatLimitPolicy::from_raw(&p).is_ok());
    }

    #[test]
    fn seat_policy_rejects_duplicates_and_bad_numbers() {
        let mut p = raw(None, None);
        p.seat.push(seat_raw("a", Some("1G"), None));
        p.seat.push(seat_raw("a", Some("2G"), None));
        assert_eq!(
            SeatLimitPolicy::from_raw(&p),
            Err(PolicyError::DuplicateSeat("a".into()))
        );
        let mut p = raw(None, None);
        p.default.cpu_weight = Some(0);
        assert_eq!(
            SeatLimitPolicy::from_raw(&p),
            Err(PolicyError::BadCpuWeight(0))
        );
        let mut p = raw(None, None);
        p.default.tasks_max = Some(0);
        assert!(SeatLimitPolicy::from_raw(&p).is_err());
    }

    #[test]
    fn seat_plan_override_beats_default_field_by_field() {
        let mut p = raw(Some("4G"), None);
        p.default.tasks_max = Some(500);
        p.seat.push(seat_raw("big", Some("8G"), None));
        let policy = SeatLimitPolicy::from_raw(&p).unwrap();
        let plan = SeatLimitPlan::build(
            &policy,
            &doc(vec![
                attributed("a.scope", "big", None),
                attributed("b.scope", "small", None),
            ]),
        );
        assert_eq!(plan.seats[0].proposed.memory_high.unwrap().bytes(), 8 * G);
        assert_eq!(plan.seats[0].proposed.tasks_max, Some(500), "default kept");
        assert_eq!(plan.seats[0].matched_session.as_deref(), Some("big"));
        assert_eq!(plan.seats[1].proposed.memory_high.unwrap().bytes(), 4 * G);
        assert_eq!(plan.seats[1].matched_session, None);
    }

    #[test]
    fn seat_plan_multi_session_scope_takes_the_first_in_sorted_order() {
        // `zeta` is listed first in the policy and first in the scope; `alpha` sorts first.
        let mut p = raw(Some("4G"), None);
        p.seat.push(seat_raw("zeta", Some("1G"), None));
        p.seat.push(seat_raw("alpha", Some("2G"), None));
        let policy = SeatLimitPolicy::from_raw(&p).unwrap();
        let mut scope = attributed("a.scope", "zeta", None);
        scope.attributions.push(Attribution {
            session: "alpha".into(),
            socket: None,
        });
        let plan = SeatLimitPlan::build(&policy, &doc(vec![scope]));
        assert_eq!(plan.seats[0].matched_session.as_deref(), Some("alpha"));
        assert_eq!(plan.seats[0].proposed.memory_high.unwrap().bytes(), 2 * G);
    }

    #[test]
    fn seat_plan_marks_peak_over_high_and_not_at_or_under() {
        let policy = SeatLimitPolicy::from_raw(&raw(Some("4G"), None)).unwrap();
        let plan = SeatLimitPlan::build(
            &policy,
            &doc(vec![
                attributed("a.scope", "a", Some(4 * G + 1)),
                attributed("b.scope", "b", Some(4 * G)),
                attributed("c.scope", "c", None),
            ]),
        );
        let marks: Vec<_> = plan.seats.iter().map(|s| s.peak_over_high).collect();
        assert_eq!(marks, [Some(true), Some(false), None]);
        assert_eq!(plan.marked_over_high, 1);
    }

    #[test]
    fn seat_plan_reports_unknown_sessions_and_skips_unattributed_scopes() {
        let mut p = raw(Some("4G"), None);
        p.seat.push(seat_raw("ghost", Some("1G"), None));
        let policy = SeatLimitPolicy::from_raw(&p).unwrap();
        let plan = SeatLimitPlan::build(
            &policy,
            &doc(vec![
                attributed("a.scope", "a", None),
                SeatSample::empty("idle.scope"),
            ]),
        );
        assert_eq!(plan.unknown_sessions, ["ghost"]);
        assert_eq!(plan.seats.len(), 1);
    }

    #[test]
    fn seat_plan_ceiling_counts_attributed_against_the_maximum() {
        let mut p = raw(None, None);
        p.host.max_attributed_seats = Some(1);
        let policy = SeatLimitPolicy::from_raw(&p).unwrap();
        let two = doc(vec![
            attributed("a.scope", "a", None),
            attributed("b.scope", "b", None),
        ]);
        let c = SeatLimitPlan::build(&policy, &two).ceiling.unwrap();
        assert_eq!((c.attributed, c.max_attributed_seats, c.over), (2, 1, true));
        let one = doc(vec![attributed("a.scope", "a", None)]);
        assert!(!SeatLimitPlan::build(&policy, &one).ceiling.unwrap().over);
        let none = SeatLimitPolicy::from_raw(&raw(None, None)).unwrap();
        assert!(SeatLimitPlan::build(&none, &one).ceiling.is_none());
    }

    #[test]
    fn seat_plan_carries_schema_and_the_no_apply_notice() {
        let policy = SeatLimitPolicy::from_raw(&raw(None, None)).unwrap();
        let plan = SeatLimitPlan::build(&policy, &doc(vec![]));
        assert_eq!(plan.schema, "uas.seats.plan.v1");
        assert!(plan.notice.contains("applied nothing"));
        assert!(plan.notice.contains("seat launch"));
        let back: SeatLimitPlan =
            serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert_eq!(back, plan);
    }
}
