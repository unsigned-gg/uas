# Specs

This directory holds the LeanSpec registry. Every spec is one file.
Methodology: LeanSpec. The reference copy is in `../vendor/leanspec/`.

| Spec | Title | Status | Priority |
|---|---|---|---|
| [0001](0001-cas-architecture-and-crate-layout.md) | CAS architecture and crate layout | in-progress | high |
| [0002](0002-verification-gates-and-evidence.md) | Verification gates and evidence | in-progress | high |
| [0003](0003-branch-model-and-promotion.md) | Branch model and promotion | in-progress | high |
| [0004](0004-dependency-and-licence-policy.md) | Dependency and licence policy | in-progress | high |
| [0005](0005-ci-cache-and-runners.md) | CI, cache and runners | in-progress | high |
| [0006](0006-cas-engine-and-harness-dogfood.md) | CAS engine and harness dogfood | complete | high |
| [0007](0007-lease-rules.md) | Lease rules | complete | high |
| [0008](0008-soma-cortex-saga-reverie-integration.md) | Soma Cortex Saga Reverie integration | complete | high |
| [0009](0009-saga-compensation-runner-and-durability.md) | Saga compensation runner and durability | complete | high |
| [0010](0010-landlock-ruleset-and-abi-probe.md) | Landlock ruleset and ABI probe | complete | high |

## How to add a spec

1. Take the next free number. Use four digits. Example: `0006`.
2. Copy the template from `../vendor/leanspec/spec-template.md`.
3. Name the file `NNNN-kebab-slug.md`. Make the H1 read `# NNNN-kebab-slug`.
4. Keep the sections in this order: Overview, Design, Plan, Test, Notes.
5. Put one runnable command in a fenced block under Test.
6. Keep the file under 300 lines. Split a long spec into sub-spec files.
   See `../vendor/leanspec/sub-spec-files.md`.
7. Add one row to the table above, with a relative link.
8. Run `make spec-check`. It must pass.
