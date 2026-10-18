// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas seats`: a read-only inventory of agent seat scopes (spec 0058).
//!
//! tmux is asked for panes with one `list-panes` call. The binary path is an argument, so tests
//! use a fake. Nothing here writes.

use std::os::unix::fs::MetadataExt;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use uas_domain::{HeadroomGate, ScopeKind, SeatSample, SeatsDocument, TmuxStatus};
use uas_pool::seats::{read_headroom, read_scopes, scope_of_pid, user_service_dir};

/// Inputs of one inventory run.
#[derive(Debug, Clone)]
pub struct SeatsConfig {
    /// The cgroup v2 mount.
    pub cgroup_root: PathBuf,
    /// The proc mount.
    pub proc_root: PathBuf,
    /// The tmux binary.
    pub tmux_bin: PathBuf,
    /// The uid whose user service cgroup is read; `None` means the owner of `<proc>/self`.
    pub uid: Option<u32>,
    /// Gate ceiling for the 1-minute load.
    pub max_load_1m: f64,
    /// Gate floor for `MemAvailable`, in GiB.
    pub min_mem_gib: f64,
}

/// One tmux pane: its pid and its session name.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Pane {
    /// `#{pane_pid}`.
    pub pid: u32,
    /// `#{session_name}`.
    pub session: String,
}

/// Parse `#{pane_pid} #{session_name}` lines. A line that does not parse is skipped.
pub fn parse_panes(text: &str) -> Vec<Pane> {
    text.lines()
        .filter_map(|line| {
            let (pid, session) = line.split_once(' ')?;
            Some(Pane {
                pid: pid.parse().ok()?,
                session: session.to_string(),
            })
        })
        .collect()
}

/// Ask tmux for every pane. The error text is the reason attribution is unavailable.
pub fn list_panes(tmux_bin: &Path) -> Result<Vec<Pane>, String> {
    let out = Command::new(tmux_bin)
        .args(["list-panes", "-a", "-F", "#{pane_pid} #{session_name}"])
        .output()
        .map_err(|e| format!("cannot run tmux: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tmux exited with {}: {}", out.status, err.trim()));
    }
    Ok(parse_panes(&String::from_utf8_lossy(&out.stdout)))
}

/// Label each scope with its pane's session name (REQ-0058-03). Scopes with no pane stay as
/// they are (REQ-0058-04).
pub fn attribute(scopes: &mut [SeatSample], panes: &[Pane], proc_root: &Path) {
    for pane in panes {
        let Some(scope) = scope_of_pid(proc_root, pane.pid) else {
            continue;
        };
        if let Some(s) = scopes.iter_mut().find(|s| s.scope == scope) {
            s.session.get_or_insert_with(|| pane.session.clone());
        }
    }
}

/// Build the inventory document. Reads only.
pub fn build_document(cfg: &SeatsConfig) -> Result<SeatsDocument> {
    let uid = match cfg.uid {
        Some(uid) => uid,
        None => std::fs::metadata(cfg.proc_root.join("self"))
            .with_context(|| format!("cannot find the uid from {}/self", cfg.proc_root.display()))?
            .uid(),
    };
    let dir = user_service_dir(&cfg.cgroup_root, uid);
    let mut scopes = read_scopes(&dir)?;
    let tmux = match list_panes(&cfg.tmux_bin) {
        Ok(panes) => {
            attribute(&mut scopes, &panes, &cfg.proc_root);
            TmuxStatus {
                available: true,
                reason: None,
            }
        }
        Err(reason) => TmuxStatus {
            available: false,
            reason: Some(reason),
        },
    };
    // Whole GiB to bytes; a fractional flag value rounds down to the byte.
    let min_bytes = (cfg.min_mem_gib * 1024.0 * 1024.0 * 1024.0) as u64;
    let headroom: HeadroomGate = read_headroom(&cfg.proc_root, cfg.max_load_1m, min_bytes);
    Ok(SeatsDocument::new(headroom, tmux, scopes))
}

fn opt<T: std::fmt::Display>(v: Option<T>) -> String {
    v.map_or_else(|| "-".to_string(), |x| x.to_string())
}

fn limit(v: Option<uas_domain::Limit>) -> String {
    match v {
        None => "-".to_string(),
        Some(uas_domain::Limit::Max) => "max".to_string(),
        Some(uas_domain::Limit::Value(n)) => n.to_string(),
    }
}

/// A plain-text table for people.
pub fn render_text(doc: &SeatsDocument) -> String {
    let h = &doc.headroom;
    let mut out = format!(
        "headroom: {} (load1 {} < {}, mem available {} >= {} bytes)\n",
        if h.passes { "pass" } else { "FAIL" },
        opt(h.load_1m),
        h.max_load_1m,
        opt(h.mem_available_bytes),
        h.min_mem_available_bytes,
    );
    if let Some(reason) = &doc.tmux.reason {
        out.push_str(&format!("tmux: unavailable ({reason})\n"));
    }
    out.push_str("session\tscope\tmem_cur\tmem_high\tmem_max\tpids\tpids_max\tmem_psi\tcpu_psi\n");
    for s in &doc.scopes {
        let label = match (&s.session, s.kind) {
            (Some(name), _) => name.as_str(),
            (None, ScopeKind::TmuxPane) => "(unattributed)",
            (None, ScopeKind::Other) => "(not a pane)",
        };
        out.push_str(&format!(
            "{label}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\t{}\n",
            s.scope,
            opt(s.memory_current),
            limit(s.memory_high),
            limit(s.memory_max),
            opt(s.pids_current),
            limit(s.pids_max),
            opt(s.memory_pressure_some_avg10),
            opt(s.cpu_pressure_some_avg10),
        ));
    }
    out
}
