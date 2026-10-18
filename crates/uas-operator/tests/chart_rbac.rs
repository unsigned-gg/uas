// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Regression guard: the chart's operator `ClusterRole` must name the CRD API group the
//! code serves and grant only what the watch loop calls (spec 0038, CER-2991).

use std::fs;
use std::path::PathBuf;

use uas_operator::API_GROUP;

/// The chart text without `#` comment lines, so a comment cannot satisfy or break a check.
fn rbac_template() -> String {
    strip_comments(&raw_rbac_template())
}

fn strip_comments(text: &str) -> String {
    text.lines()
        .filter(|l| !l.trim_start().starts_with('#'))
        .collect::<Vec<_>>()
        .join("\n")
}

/// The `(resources, verbs)` lines of every rule in the `rules:` list, as written.
fn rules(text: &str) -> Vec<(String, String)> {
    let body = text.split("rules:").nth(1).expect("rules: present");
    let body = body.split("\n---").next().unwrap_or(body);
    let mut out = Vec::new();
    let (mut res, mut verbs) = (String::new(), String::new());
    for line in body.lines().map(str::trim) {
        if line.starts_with("- apiGroups:") && !res.is_empty() {
            out.push((std::mem::take(&mut res), std::mem::take(&mut verbs)));
        }
        if let Some(r) = line.strip_prefix("resources:") {
            res = r.trim().to_string();
        }
        if let Some(v) = line.strip_prefix("verbs:") {
            verbs = v.trim().to_string();
        }
    }
    out.push((res, verbs));
    out
}

fn raw_rbac_template() -> String {
    let path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../charts/uas/templates/rbac.yaml");
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

#[test]
fn cluster_role_uses_the_crd_api_group() {
    let rbac = rbac_template();
    assert!(
        rbac.contains(&format!("apiGroups: [\"{API_GROUP}\"]")),
        "ClusterRole must grant API group {API_GROUP}"
    );
    assert!(
        !rbac.contains("uas.cerebral.work"),
        "stale API group uas.cerebral.work must not appear"
    );
}

#[test]
fn cluster_role_grants_the_watched_crd_resource_and_its_status() {
    let rbac = rbac_template();
    // `run_k8s_watch_loop` watches WorkerSlot and patches its status subresource.
    for resource in ["workerslots", "workerslots/status"] {
        assert!(rbac.contains(resource), "missing resource {resource}");
    }
}

#[test]
fn rules_have_exactly_the_expected_verbs_and_no_wildcards() {
    let parsed = rules(&rbac_template());
    let expect = [
        ("[\"workerslots\"]", "[\"get\", \"list\", \"watch\"]"),
        ("[\"workerslots/status\"]", "[\"patch\"]"),
    ];
    assert_eq!(
        parsed.len(),
        expect.len(),
        "unexpected rule count: {parsed:?}"
    );
    for (got, want) in parsed.iter().zip(expect) {
        assert_eq!((got.0.as_str(), got.1.as_str()), want);
    }
    assert!(!rbac_template().contains("\"*\""), "wildcard grant present");
}

#[test]
fn comments_do_not_count() {
    let text = strip_comments("# uas.cerebral.work\n  - apiGroups: [\"x\"]\n    # verbs: [\"*\"]");
    assert!(!text.contains("cerebral") && !text.contains('*'));
}

#[test]
fn cluster_role_has_no_unused_grants() {
    let rbac = rbac_template();
    for unused in [
        "sandboxes",
        "snapshotlineages",
        "egressprofiles",
        "secretgrants",
        "pods",
        "services",
        "configmaps",
        "networkpolicies",
        "externalsecrets",
        "events",
        "leases",
    ] {
        assert!(!rbac.contains(unused), "unused grant present: {unused}");
    }
}

#[test]
fn daemon_service_account_is_bound() {
    let rbac = rbac_template();
    assert!(
        rbac.contains(
            "  - kind: ServiceAccount\n    name: {{ include \"uas.fullname\" . }}-daemon"
        ),
        "daemon ServiceAccount needs its own binding (node-agent mode runs the same watch loop)"
    );
}
