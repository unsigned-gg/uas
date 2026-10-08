# cas-ebpf

Aya eBPF tracepoint programs: build-priority clamping and audit telemetry.

| | |
|---|---|
| Status | Placeholder. No code yet. |
| Roadmap phase | Phase 2 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md §3.3` |
| Will depend on | cas-domain |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.

## Open questions

- The research names autonice (neutral-engineering, GPL-2.0) as the model. Do not copy
  its code: this repo is proprietary and `deny.toml` denies GPL. Re-implement on Aya.
- An eBPF program that calls GPL-only kernel helpers must declare a GPL-compatible
  licence in its `license` section. Decide which helpers are needed, and record the
  licence position in `specs/0004-dependency-and-licence-policy.md` before the first program.
- The eBPF side is `#![no_std]` and builds for the `bpfel-unknown-none` target. That
  needs a nightly toolchain and `bpf-linker`, which the ci-rust image does not have.
