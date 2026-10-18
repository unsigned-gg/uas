// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas seats`: a read-only inventory of agent seat scopes (spec 0058).
//!
//! tmux is asked for panes with one `list-panes` call. The binary path is an argument, so tests
//! use a fake. Nothing here writes.

use std::os::unix::fs::{FileTypeExt, MetadataExt};
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{Context, Result};
use uas_domain::{
    Attribution, HeadroomGate, ScopeKind, SeatSample, SeatsDocument, SocketCounts, SocketOutcome,
    SocketStatus, TmuxStatus,
};
use uas_pool::seats::{
    probe_socket, read_headroom, read_scopes, scope_of_pid, user_service_dir, SocketProbe,
};

/// Inputs of one inventory run.
#[derive(Debug, Clone)]
pub struct SeatsConfig {
    /// The cgroup v2 mount.
    pub cgroup_root: PathBuf,
    /// The proc mount.
    pub proc_root: PathBuf,
    /// The tmux binary.
    pub tmux_bin: PathBuf,
    /// The directory that holds `tmux-<uid>/`: `$TMUX_TMPDIR`, else `/tmp`.
    pub tmux_socket_base: PathBuf,
    /// The uid whose user service cgroup is read; `None` means the owner of `<proc>/self`.
    pub uid: Option<u32>,
    /// The most live sockets queried with tmux in one run; the rest are counted and skipped.
    pub max_tmux_sockets: usize,
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

/// The socket label of the server tmux starts without `-L` or `-S`.
const DEFAULT_SOCKET: &str = "default";

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

/// Skip reason for a socket that refuses a connection.
const STALE_REASON: &str = "stale socket (connect refused)";

/// Skip reason for a socket whose accept backlog is full.
const BUSY_REASON: &str = "busy (backlog full)";

/// Probe `socket` without starting tmux (REQ-0058-15). The connect itself lives in `uas-pool`.
fn probe(socket: &Path) -> Result<(), (SocketOutcome, String)> {
    match probe_socket(socket) {
        SocketProbe::Live => Ok(()),
        SocketProbe::Stale => Err((SocketOutcome::Stale, STALE_REASON.to_string())),
        SocketProbe::Busy => Err((SocketOutcome::Skipped, BUSY_REASON.to_string())),
        SocketProbe::Other(reason) => Err((SocketOutcome::Skipped, reason)),
    }
}

/// Ask the tmux server on `socket` for every pane, with argv only. The error text is the
/// reason the socket was skipped.
pub fn list_panes(tmux_bin: &Path, socket: &Path) -> Result<Vec<Pane>, String> {
    let out = Command::new(tmux_bin)
        .arg("-S")
        .arg(socket)
        .args(["list-panes", "-a", "-F", "#{pane_pid} #{session_name}"])
        .output()
        .map_err(|e| format!("cannot run tmux: {e}"))?;
    if !out.status.success() {
        let err = String::from_utf8_lossy(&out.stderr);
        return Err(format!("tmux exited with {}: {}", out.status, err.trim()));
    }
    Ok(parse_panes(&String::from_utf8_lossy(&out.stdout)))
}

/// The sockets in `<base>/tmux-<uid>/`, sorted by name. Only entries that are sockets count.
pub fn find_sockets(base: &Path, uid: u32) -> Vec<PathBuf> {
    let Ok(entries) = std::fs::read_dir(base.join(format!("tmux-{uid}"))) else {
        return Vec::new();
    };
    let mut found: Vec<PathBuf> = entries
        .filter_map(Result::ok)
        .filter(|e| e.file_type().is_ok_and(|t| t.is_socket()))
        .map(|e| e.path())
        .collect();
    // `default` first, then by name, so the cap drops the same sockets every run.
    found.sort_by_key(|p| {
        let name = p.file_name().map(|n| n.to_os_string());
        (
            name.as_deref() != Some(std::ffi::OsStr::new(DEFAULT_SOCKET)),
            name,
        )
    });
    found
}

/// Add each pane's session to the scope that holds its pid (REQ-0058-03). Scopes with no pane
/// stay as they are (REQ-0058-04). `socket` is the label: `None` for the default socket.
pub fn attribute(
    scopes: &mut [SeatSample],
    panes: &[Pane],
    socket: Option<&str>,
    proc_root: &Path,
) {
    for pane in panes {
        let Some(scope) = scope_of_pid(proc_root, pane.pid) else {
            continue;
        };
        if let Some(s) = scopes.iter_mut().find(|s| s.scope == scope) {
            s.attributions.push(Attribution {
                session: pane.session.clone(),
                socket: socket.map(str::to_string),
            });
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
    let sockets = find_sockets(&cfg.tmux_socket_base, uid);
    let mut counts = SocketCounts {
        found: sockets.len(),
        ..SocketCounts::default()
    };
    let mut statuses = Vec::new();
    let mut spawned = 0;
    // Every socket is probed: a connect starts no process. Only a tmux spawn is capped.
    for socket in &sockets {
        let name = socket
            .file_name()
            .map_or_else(String::new, |n| n.to_string_lossy().into_owned());
        if let Err((outcome, reason)) = probe(socket) {
            if outcome == SocketOutcome::Stale {
                counts.stale += 1;
            } else {
                counts.other_skips += 1;
                statuses.push(SocketStatus {
                    socket: name,
                    outcome,
                    reason: Some(reason),
                });
            }
            continue;
        }
        counts.live += 1;
        if spawned >= cfg.max_tmux_sockets {
            counts.over_cap += 1;
            continue;
        }
        spawned += 1;
        match list_panes(&cfg.tmux_bin, socket) {
            Ok(panes) => {
                let label = (name != DEFAULT_SOCKET).then_some(name.as_str());
                attribute(&mut scopes, &panes, label, &cfg.proc_root);
                counts.queried += 1;
                statuses.push(SocketStatus {
                    socket: name,
                    outcome: SocketOutcome::Queried,
                    reason: None,
                });
            }
            Err(reason) => {
                counts.other_skips += 1;
                statuses.push(SocketStatus {
                    socket: name,
                    outcome: SocketOutcome::Skipped,
                    reason: Some(reason),
                });
            }
        }
    }
    let over_cap_reason = (counts.over_cap > 0).then(|| {
        format!(
            "{} live sockets not queried: over the cap of {} tmux queries (--max-tmux-sockets)",
            counts.over_cap, cfg.max_tmux_sockets
        )
    });
    for s in &mut scopes {
        s.attributions.sort();
        s.attributions.dedup();
    }
    let available = counts.queried > 0;
    let reason = (!available).then(|| {
        if counts.found == 0 {
            format!(
                "no tmux sockets in {}",
                cfg.tmux_socket_base.join(format!("tmux-{uid}")).display()
            )
        } else if counts.stale == counts.found {
            format!("{} sockets found, all stale (connect refused)", counts.found)
        } else {
            format!(
                "{} sockets found, none queried: {} stale (connect refused), {} other skips, {} live over the cap",
                counts.found, counts.stale, counts.other_skips, counts.over_cap
            )
        }
    });
    let tmux = TmuxStatus {
        available,
        reason,
        sockets: statuses,
        counts,
        over_cap_reason,
    };
    // A fractional GiB flag value rounds down to the byte.
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
    let c = &doc.tmux.counts;
    out.push_str(&format!(
        "tmux sockets: {} found, {} live, {} queried, {} stale, {} other skips, {} live over the cap\n",
        c.found, c.live, c.queried, c.stale, c.other_skips, c.over_cap,
    ));
    for s in doc
        .tmux
        .sockets
        .iter()
        .filter(|s| s.outcome == SocketOutcome::Skipped)
    {
        out.push_str(&format!(
            "  skipped {}: {}\n",
            s.socket,
            s.reason.as_deref().unwrap_or("unknown")
        ));
    }
    if let Some(reason) = &doc.tmux.over_cap_reason {
        out.push_str(&format!("  {reason}\n"));
    }
    if let Some(reason) = &doc.tmux.reason {
        out.push_str(&format!("tmux: unavailable ({reason})\n"));
    }
    let m = &doc.summary;
    out.push_str(&format!(
        "scopes: {} total, {} attributed, {} unattributed with processes, {} empty, {} unattributed unknown\n",
        doc.scopes.len(), m.attributed, m.unattributed_with_processes, m.empty, m.unattributed_unknown
    ));
    out.push_str("session\tscope\tmem_cur\tmem_high\tmem_max\tpids\tpids_max\tmem_psi\tcpu_psi\n");
    for s in &doc.scopes {
        let label = if s.attributions.is_empty() {
            match s.kind {
                ScopeKind::TmuxPane => "(unattributed)".to_string(),
                ScopeKind::Other => "(not a pane)".to_string(),
            }
        } else {
            s.attributions
                .iter()
                .map(|a| match &a.socket {
                    Some(sock) => format!("{}[{sock}]", a.session),
                    None => a.session.clone(),
                })
                .collect::<Vec<_>>()
                .join(",")
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
