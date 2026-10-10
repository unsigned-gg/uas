# Cluster survey atlas

This page maps where each signal about this repo's work lives on the estate. It is a map of where to look. It does not replace a live probe. Date every number you add.

Per `<home>/.agents/sops/engineering-subdir-and-subagent-gruntwork.md`, a subagent fills this page from a surface file. A read-only probe fills the gaps.

## Observability stores

Not probed yet. On 2026-10-08 this repo runs no service. It has no telemetry to read.

## Dashboards

Not probed yet.

## Telemetry paths

Not probed yet.

## Data lakes and archives

Not probed yet.

## CI and runners

Known from the repo contract (2026-10-08):
- CI runs on the `ci-rust` runner label in runner group `ent-default`.
- The runner image is `ci-rust-runner:v0.1.6`. It bakes Rust 1.94.1.
- Spec 0005 (`specs/0005-ci-cache-and-runners.md`) is the source for runner lanes and caches.

Not probed yet: live runner capacity, cache hit rates, the R2 sccache bucket.

## Where the UAS design lives

Measured 2026-10-08. All paths are in `cerebral-work/research@4068134`. The design is a draft.

| Need | Path |
|---|---|
| Design, pillars, crate layout, roadmap | `agentic-systems/designs/rust-native-agent-substrate.md` |
| Layer diagram, invariants, failure matrix | `agentic-systems/designs/substrate-diagrams-and-components.md` |
| Estate topology and invariants | `SYNTHESIS-ESTATE-AND-SUBSTRATE.md` |
| Per-project evaluations | `repos/{gitoxide,heph,autumn-harvest,neutral-engineering,zerostack,madstack}.md` |

Signals UAS will need once it runs, each not probed yet: the cortex settle queue
(`/v1/settle/pending`), the saga evidence store, the reverie memory daemon and the
Autumn-Harvest event log. This repo holds no data for any of them.

## What this atlas does not hold

- Live state. Probe before you act.
- Secrets. Names only, never values.

## Known traps

None recorded yet.

## Not probed yet

Every section above that says "Not probed yet".
