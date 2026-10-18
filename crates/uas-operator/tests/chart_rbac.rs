// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Regression guard: the chart's operator `ClusterRole` must name the CRD API group the
//! code serves and grant only what the watch loop calls (spec 0038, CER-2991).

use std::fs;
use std::path::PathBuf;

use uas_operator::API_GROUP;

fn rbac_template() -> String {
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
