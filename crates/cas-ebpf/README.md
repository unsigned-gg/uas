# cas-ebpf

Aya eBPF tracepoint programs: build-priority clamping and audit telemetry.

| | |
|---|---|
| Status | Skeleton and BPF bytecode artifact. |
| Roadmap phase | Phase 2 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md section 3.3` |
| Spec | `specs/0014-ebpf-priority-clamping-and-audit-probes.md` |
| Will depend on | cas-domain |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers. Add the Cargo dependency when the first
real code needs it, not before.

## Architecture and Decisions

- **Clean-room Aya implementation:** Built with `aya` 0.14.0 and `aya-ebpf` 0.2.1
  (MIT OR Apache-2.0). Zero code copied or imported from `neutral-engineering/autonice` (GPL-2.0).
- **Licensing boundary:** Decided by Christian on 2026-10-09: the kernel-side
  BPF program object declares `Dual MIT/GPL` in its `license` section. This satisfies
  Linux kernel verifier requirements for GPL-compatible helpers. The userspace
  loader and crate remain proprietary (`LicenseRef-Proprietary`).
- **Priority clamping scope & method:** Worker process trees only (no host-wide autonice).
  `cas-pool` launches workers in dedicated process groups with `setpriority(PRIO_PGRP, pgid, 19)`.
  The eBPF tracepoint probe monitors process trees and re-clamps any process that escapes
  its group via `setsid()` or `setpgid()`.
- **Audit telemetry (`run_hash`):** Streams structured 168-byte `ExecEvent` records
  over a 256 KiB ring buffer from `sched_process_exec` and `sched_process_exit`. Computes
  an incremental SHA-256 transcript digest for Saga execution receipts.
- **Privileged daemon (`cas-ebpfd`):** Root systemd unit runs with `CAP_BPF`, `CAP_PERFMON`,
  and `CAP_SYS_NICE`. It owns maps and ring buffers, serving unprivileged `cas-pool`
  over `/run/cas/ebpf.sock`.
- **Toolchain & Artifact Packaging:** The kernel BPF program lives in `crates/cas-ebpf/ebpf/`
  outside `[workspace.members]`. Pre-compiled bytecode `cas-ebpf.o` is committed in git
  with reproducible build target `make ebpf-build`. The userspace crate builds on stable
  Rust 1.94.1 with `-j 2`.
