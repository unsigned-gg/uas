// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Read-only readers for the seat inventory (spec 0058).
//!
//! The cgroup root and the proc root are arguments, so tests use fixture trees. This module
//! only reads. It never writes a file and never opens a per-process secrets-bearing file
//! (REQ-0058-02, REQ-0058-06).

use std::fs;
use std::path::{Path, PathBuf};

use uas_domain::{CpuMax, HeadroomGate, Limit, SeatSample};

/// Errors from listing the scopes.
#[derive(Debug, thiserror::Error)]
pub enum SeatError {
    /// The user service cgroup directory could not be listed.
    #[error("cannot list {path}: {source}")]
    List {
        /// The directory.
        path: PathBuf,
        /// The underlying error.
        #[source]
        source: std::io::Error,
    },
}

/// The user service cgroup of `uid` under `cgroup_root` (D2).
pub fn user_service_dir(cgroup_root: &Path, uid: u32) -> PathBuf {
    cgroup_root
        .join("user.slice")
        .join(format!("user-{uid}.slice"))
        .join(format!("user@{uid}.service"))
}

/// Sample every direct child scope of `service_dir`, sorted by name (REQ-0058-01).
///
/// A missing or unreadable interface file leaves that field `None` (REQ-0058-05).
pub fn read_scopes(service_dir: &Path) -> Result<Vec<SeatSample>, SeatError> {
    let entries = fs::read_dir(service_dir).map_err(|source| SeatError::List {
        path: service_dir.to_path_buf(),
        source,
    })?;
    let mut names: Vec<String> = entries
        .filter_map(Result::ok)
        .filter(|e| e.path().is_dir())
        .filter_map(|e| e.file_name().into_string().ok())
        .filter(|n| n.ends_with(".scope"))
        .collect();
    names.sort();
    Ok(names
        .iter()
        .map(|name| sample_scope(&service_dir.join(name), name))
        .collect())
}

fn read_trimmed(dir: &Path, file: &str) -> Option<String> {
    fs::read_to_string(dir.join(file))
        .ok()
        .map(|t| t.trim().to_string())
}

fn read_u64(dir: &Path, file: &str) -> Option<u64> {
    read_trimmed(dir, file)?.parse().ok()
}

fn read_limit(dir: &Path, file: &str) -> Option<Limit> {
    Limit::parse(&read_trimmed(dir, file)?)
}

/// `some avg10=<f>` from a pressure file's text.
pub fn parse_pressure_some_avg10(text: &str) -> Option<f64> {
    let line = text.lines().find(|l| l.starts_with("some "))?;
    line.split_whitespace()
        .find_map(|p| p.strip_prefix("avg10="))?
        .parse()
        .ok()
}

fn sample_scope(dir: &Path, name: &str) -> SeatSample {
    let mut s = SeatSample::empty(name);
    s.memory_current = read_u64(dir, "memory.current");
    s.memory_peak = read_u64(dir, "memory.peak");
    s.memory_high = read_limit(dir, "memory.high");
    s.memory_max = read_limit(dir, "memory.max");
    s.pids_current = read_u64(dir, "pids.current");
    s.pids_max = read_limit(dir, "pids.max");
    s.cpu_max = read_trimmed(dir, "cpu.max").and_then(|t| CpuMax::parse(&t));
    s.cpu_weight = read_u64(dir, "cpu.weight");
    s.memory_pressure_some_avg10 = fs::read_to_string(dir.join("memory.pressure"))
        .ok()
        .and_then(|t| parse_pressure_some_avg10(&t));
    s.cpu_pressure_some_avg10 = fs::read_to_string(dir.join("cpu.pressure"))
        .ok()
        .and_then(|t| parse_pressure_some_avg10(&t));
    s
}

/// The scope directory name that holds `pid`, read from `<proc_root>/<pid>/cgroup` (D3).
///
/// Returns the path component after `user@<uid>.service` when it ends in `.scope`.
pub fn scope_of_pid(proc_root: &Path, pid: u32) -> Option<String> {
    let text = fs::read_to_string(proc_root.join(pid.to_string()).join("cgroup")).ok()?;
    text.lines().find_map(|line| {
        let path = line.strip_prefix("0::")?;
        let mut parts = path.split('/');
        parts.find(|p| p.starts_with("user@") && p.ends_with(".service"))?;
        let scope = parts.next()?;
        scope.ends_with(".scope").then(|| scope.to_string())
    })
}

/// Read the 1-minute load and `MemAvailable` under `proc_root` and evaluate the gate.
pub fn read_headroom(proc_root: &Path, max_load_1m: f64, min_mem_bytes: u64) -> HeadroomGate {
    let load = fs::read_to_string(proc_root.join("loadavg"))
        .ok()
        .and_then(|t| t.split_whitespace().next()?.parse::<f64>().ok());
    let mem = fs::read_to_string(proc_root.join("meminfo"))
        .ok()
        .and_then(|t| parse_mem_available(&t));
    HeadroomGate::evaluate(load, mem, max_load_1m, min_mem_bytes)
}

/// `MemAvailable` in bytes from `/proc/meminfo` text (the file reports kB).
pub fn parse_mem_available(text: &str) -> Option<u64> {
    let rest = text.lines().find_map(|l| l.strip_prefix("MemAvailable:"))?;
    let kb: u64 = rest.split_whitespace().next()?.parse().ok()?;
    kb.checked_mul(1024)
}
