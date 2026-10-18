// SPDX-License-Identifier: Apache-2.0 OR MIT
//! `uas seats plan`: print the limits a policy would set on each attributed seat (spec 0058).
//!
//! The plan is a dry run (D1). This module reads the policy file and the inventory, and
//! returns text. It sets nothing and prints nothing; `main.rs` prints.

use std::path::Path;

use anyhow::{Context, Result};
use uas_domain::{
    RawPolicy, SeatLimitPlan, SeatLimitPolicy, SeatLimits, SeatsDocument, NO_APPLY_NOTICE,
};

use crate::seats::{build_document, SeatsOpts};

/// Read and validate a policy file.
pub fn load_policy(path: &Path) -> Result<SeatLimitPolicy> {
    let text = std::fs::read_to_string(path)
        .with_context(|| format!("cannot read policy {}", path.display()))?;
    let raw: RawPolicy =
        toml::from_str(&text).with_context(|| format!("cannot parse policy {}", path.display()))?;
    SeatLimitPolicy::from_raw(&raw).with_context(|| format!("invalid policy {}", path.display()))
}

/// Build the inventory and the plan for `policy`.
pub fn build(policy: &SeatLimitPolicy, opts: &SeatsOpts) -> Result<(SeatsDocument, SeatLimitPlan)> {
    let doc = build_document(&opts.config(Some(&policy.host)))?;
    let plan = SeatLimitPlan::build(policy, &doc);
    Ok((doc, plan))
}

/// The systemd properties a launcher would set, one `Name=value` each.
fn properties(l: &SeatLimits) -> String {
    let mut p = Vec::new();
    if let Some(v) = l.memory_high {
        p.push(format!("MemoryHigh={v}"));
    }
    if let Some(v) = l.memory_max {
        p.push(format!("MemoryMax={v}"));
    }
    if let Some(v) = l.tasks_max {
        p.push(format!("TasksMax={v}"));
    }
    if let Some(v) = l.cpu_weight {
        p.push(format!("CPUWeight={v}"));
    }
    if p.is_empty() {
        "(no limits proposed)".to_string()
    } else {
        p.join(" ")
    }
}

fn bytes(v: Option<u64>) -> String {
    v.map_or_else(
        || "-".to_string(),
        |b| {
            uas_domain::Size::parse("size", &b.to_string())
                .map_or_else(|_| b.to_string(), |s| s.to_string())
        },
    )
}

/// The human plan.
pub fn render_text(plan: &SeatLimitPlan) -> String {
    let mut out = format!("{NO_APPLY_NOTICE}\n");
    let h = &plan.headroom;
    out.push_str(&format!(
        "headroom: {}\n",
        if h.passes { "pass" } else { "FAIL" }
    ));
    if let Some(c) = &plan.ceiling {
        out.push_str(&format!(
            "seat ceiling: {} attributed of {} allowed: {}\n",
            c.attributed,
            c.max_attributed_seats,
            if c.over { "OVER" } else { "within" }
        ));
    }
    out.push_str(&format!(
        "seats: {}, peak over MemoryHigh: {}\n",
        plan.seats.len(),
        plan.marked_over_high
    ));
    for s in &plan.unknown_sessions {
        out.push_str(&format!("unknown session in policy: {s}\n"));
    }
    out.push_str("session\tpeak\tcurrent\tproposed\tmark\n");
    for s in &plan.seats {
        let names: Vec<&str> = s.attributions.iter().map(|a| a.session.as_str()).collect();
        let mark = match s.peak_over_high {
            Some(true) => "PEAK OVER MemoryHigh",
            Some(false) => "ok",
            None => "unknown",
        };
        out.push_str(&format!(
            "{}\t{}\t{}\t{}\t{mark}\n",
            names.join(","),
            bytes(s.memory_peak),
            bytes(s.memory_current),
            properties(&s.proposed),
        ));
    }
    out
}

/// Run the command and return what to print: the plan as text or JSON.
pub fn render(policy_path: &Path, opts: &SeatsOpts) -> Result<String> {
    let policy = load_policy(policy_path)?;
    let (_, plan) = build(&policy, opts)?;
    if opts.json {
        Ok(format!("{}\n", serde_json::to_string_pretty(&plan)?))
    } else {
        Ok(render_text(&plan))
    }
}
