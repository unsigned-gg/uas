// SPDX-License-Identifier: Apache-2.0 OR MIT
//! Kubernetes operator and node agent for the Unsigned Agent Substrate (spec 0038).
//!
//! Exposes the `WorkerSlot` and `AgentSandbox` custom resource definitions in
//! group `uas.unsigned.gg/v1alpha1` together with their deterministic
//! reconciler, Spec 0036 host slot lock table, and Spec 0041 resource limit
//! adapter.

#![forbid(unsafe_code)]
#![deny(missing_docs)]

pub mod crd;
pub mod reconciler;

pub use crd::{
    AgentSandbox, AgentSandboxSpec, AgentSandboxStatus, ConditionStatus, ObjectMeta,
    SandboxAccessMode, SandboxRuleSpec, SlotLimitsSpec, WorkerSlot, WorkerSlotSpec,
    WorkerSlotStatus, API_GROUP, API_VERSION, SLOT_CLEANUP_FINALIZER,
};
pub use reconciler::{
    planned_rules, pool_config_for_slot, resolve_slot_limits_with_probe, HostSlotLockTable,
    OperatorMode, OperatorReconciler, ReconcileDecision, ReconcileError,
    CAPACITY_EXHAUSTED_REQUEUE, STEADY_REQUEUE,
};

/// Emit the OpenAPI v3 Kubernetes `CustomResourceDefinition` documents for
/// [`WorkerSlot`] and [`AgentSandbox`] as a JSON array (spec 0038 REQ-0038-12).
#[cfg(feature = "k8s")]
pub fn crd_documents_json() -> Result<String, serde_json::Error> {
    use kube::CustomResourceExt;

    let docs = vec![WorkerSlot::crd(), AgentSandbox::crd()];
    serde_json::to_string_pretty(&docs)
}

/// Run the live Kubernetes controller watch loop for [`WorkerSlot`] and
/// [`AgentSandbox`] resources in `namespace` (spec 0038 D1, D8).
#[cfg(feature = "k8s")]
pub async fn run_k8s_watch_loop(
    reconciler: std::sync::Arc<OperatorReconciler>,
    namespace: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    use std::sync::Arc;
    use std::time::{SystemTime, UNIX_EPOCH};

    use futures_util::StreamExt;
    use kube::api::{Patch, PatchParams};
    use kube::runtime::controller::{Action, Controller};
    use kube::runtime::watcher::Config as WatcherConfig;
    use kube::{Api, Client, ResourceExt};

    let client = Client::try_default().await?;
    let slots: Api<WorkerSlot> = Api::namespaced(client.clone(), namespace);

    let ctx = Arc::new((reconciler, slots.clone()));
    Controller::new(slots, WatcherConfig::default())
        .run(
            |slot, ctx| async move {
                let (reconciler, api) = ctx.as_ref();
                let mut current = (*slot).clone();
                let now = SystemTime::now()
                    .duration_since(UNIX_EPOCH)
                    .unwrap_or_default()
                    .as_secs();
                let decision = reconciler
                    .reconcile_worker_slot(&mut current, now)
                    .map_err(|e| std::io::Error::other(e.to_string()))?;
                let name = current.name_any();
                if let Some(status) = &current.status {
                    let status_patch = serde_json::json!({ "status": status });
                    let _ = api
                        .patch_status(
                            &name,
                            &PatchParams::apply("uas-operator"),
                            &Patch::Merge(&status_patch),
                        )
                        .await;
                }
                match decision {
                    ReconcileDecision::Reconciled { requeue_after } => {
                        Ok(Action::requeue(requeue_after))
                    }
                    ReconcileDecision::RequeueCapacityExhausted { requeue_after } => {
                        Ok(Action::requeue(requeue_after))
                    }
                    ReconcileDecision::NoOp
                    | ReconcileDecision::TerminalFailure { .. }
                    | ReconcileDecision::Finalized => Ok(Action::await_change()),
                }
            },
            |_slot, err: &std::io::Error, _ctx| {
                tracing::warn!(error = %err, "worker slot reconciliation error; backing off");
                Action::requeue(CAPACITY_EXHAUSTED_REQUEUE)
            },
            ctx,
        )
        .for_each(|res| async move {
            if let Err(err) = res {
                tracing::warn!(error = %err, "controller stream item failed");
            }
        })
        .await;

    Ok(())
}
