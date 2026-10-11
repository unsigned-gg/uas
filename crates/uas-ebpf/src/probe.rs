// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Host probe: can the kernel program load and attach here? (spec 0014, facts block)
//!
//! Tests use it to probe and skip. The daemon uses it to fail with a clear
//! message before it touches the BPF syscall. Every field is a measurement
//! of this host; nothing here changes state.

use std::{fs, io, path::Path};

/// Capability bit numbers from `linux/capability.h`.
pub const CAP_SYS_NICE: u8 = 23;
/// See [`CAP_SYS_NICE`].
pub const CAP_PERFMON: u8 = 38;
/// See [`CAP_SYS_NICE`].
pub const CAP_BPF: u8 = 39;

const BTF_PATH: &str = "/sys/kernel/btf/vmlinux";
const UNPRIVILEGED_BPF_PATH: &str = "/proc/sys/kernel/unprivileged_bpf_disabled";
const TRACEFS_EVENTS: [&str; 2] = [
    "/sys/kernel/tracing/events/sched",
    "/sys/kernel/debug/tracing/events/sched",
];

/// Whether the `sched` tracepoint directory could be read.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TracepointAccess {
    /// `sched_process_exec`, `sched_process_fork` and `sched_process_exit` exist.
    Present,
    /// tracefs exists but this process may not read it (needs root or `CAP_PERFMON`).
    Unreadable,
    /// tracefs is not mounted or the sched events are absent.
    Missing,
}

/// What the probe measured.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ProbeReport {
    /// `/sys/kernel/btf/vmlinux` exists.
    pub btf_present: bool,
    /// `kernel.unprivileged_bpf_disabled`, or `None` when unreadable.
    pub unprivileged_bpf_disabled: Option<u8>,
    /// The effective capability set holds `CAP_BPF`.
    pub cap_bpf: bool,
    /// The effective capability set holds `CAP_PERFMON`.
    pub cap_perfmon: bool,
    /// The effective capability set holds `CAP_SYS_NICE`.
    pub cap_sys_nice: bool,
    /// Whether the sched tracepoints could be seen.
    pub tracepoints: TracepointAccess,
}

impl ProbeReport {
    /// `true` when loading and attaching the tracepoints should succeed.
    pub fn can_load(&self) -> bool {
        self.btf_present
            && self.cap_bpf
            && self.cap_perfmon
            && self.tracepoints != TracepointAccess::Missing
    }

    /// `true` when this process may renice processes it does not own.
    pub fn can_clamp_others(&self) -> bool {
        self.cap_sys_nice
    }

    /// One line for a skip message or a log.
    pub fn summary(&self) -> String {
        format!(
            "btf={} unprivileged_bpf_disabled={} cap_bpf={} cap_perfmon={} cap_sys_nice={} tracepoints={:?}",
            self.btf_present,
            self.unprivileged_bpf_disabled
                .map_or_else(|| "?".to_owned(), |v| v.to_string()),
            self.cap_bpf,
            self.cap_perfmon,
            self.cap_sys_nice,
            self.tracepoints
        )
    }
}

/// Measures this host and this process. Never fails: unreadable inputs read as
/// absent or unknown.
pub fn probe() -> ProbeReport {
    let caps = fs::read_to_string("/proc/self/status")
        .ok()
        .and_then(|status| parse_cap_eff(&status))
        .unwrap_or(0);
    ProbeReport {
        btf_present: Path::new(BTF_PATH).exists(),
        unprivileged_bpf_disabled: fs::read_to_string(UNPRIVILEGED_BPF_PATH)
            .ok()
            .and_then(|text| text.trim().parse().ok()),
        cap_bpf: has_cap(caps, CAP_BPF),
        cap_perfmon: has_cap(caps, CAP_PERFMON),
        cap_sys_nice: has_cap(caps, CAP_SYS_NICE),
        tracepoints: tracepoint_access(),
    }
}

/// Parses the `CapEff:` line of `/proc/<pid>/status` into a bit mask.
pub fn parse_cap_eff(status: &str) -> Option<u64> {
    status
        .lines()
        .find_map(|line| line.strip_prefix("CapEff:"))
        .and_then(|value| u64::from_str_radix(value.trim(), 16).ok())
}

/// `true` when bit `cap` is set in `mask`.
pub fn has_cap(mask: u64, cap: u8) -> bool {
    cap < 64 && mask & (1u64 << cap) != 0
}

fn tracepoint_access() -> TracepointAccess {
    let mut unreadable = false;
    for base in TRACEFS_EVENTS {
        match fs::read_dir(base) {
            Ok(_) => {
                let all = [
                    "sched_process_exec",
                    "sched_process_fork",
                    "sched_process_exit",
                ]
                .iter()
                .all(|name| Path::new(base).join(name).join("format").exists());
                return if all {
                    TracepointAccess::Present
                } else {
                    TracepointAccess::Missing
                };
            }
            Err(err) if err.kind() == io::ErrorKind::PermissionDenied => unreadable = true,
            Err(_) => {}
        }
    }
    if unreadable {
        TracepointAccess::Unreadable
    } else {
        TracepointAccess::Missing
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_cap_eff_from_status() {
        let status =
            "Name:\tcat\nCapInh:\t0000000000000000\nCapEff:\t000001ffffffffff\nCapBnd:\t0\n";
        let mask = parse_cap_eff(status).expect("CapEff line");
        assert!(has_cap(mask, CAP_BPF));
        assert!(has_cap(mask, CAP_PERFMON));
        assert!(has_cap(mask, CAP_SYS_NICE));
        assert!(!has_cap(mask, 41));
    }

    #[test]
    fn unprivileged_mask_has_no_bpf_caps() {
        let mask = parse_cap_eff("CapEff:\t0000000000000000\n").expect("line");
        assert!(!has_cap(mask, CAP_BPF));
        assert!(!has_cap(mask, CAP_SYS_NICE));
        assert!(!has_cap(mask, 64));
    }

    #[test]
    fn missing_or_garbled_line_is_none() {
        assert_eq!(parse_cap_eff("Name:\tx\n"), None);
        assert_eq!(parse_cap_eff("CapEff:\tzz\n"), None);
    }

    #[test]
    fn can_load_needs_btf_and_both_caps() {
        let report = ProbeReport {
            btf_present: true,
            unprivileged_bpf_disabled: Some(2),
            cap_bpf: true,
            cap_perfmon: true,
            cap_sys_nice: false,
            tracepoints: TracepointAccess::Unreadable,
        };
        assert!(report.can_load());
        assert!(!report.can_clamp_others());
        let no_btf = ProbeReport {
            btf_present: false,
            ..report.clone()
        };
        assert!(!no_btf.can_load());
        let no_tp = ProbeReport {
            tracepoints: TracepointAccess::Missing,
            ..report
        };
        assert!(!no_tp.can_load());
    }

    #[test]
    fn probe_runs_without_privileges() {
        let report = probe();
        let line = report.summary();
        assert!(line.starts_with("btf="));
    }
}
