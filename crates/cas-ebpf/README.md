# cas-ebpf

Aya eBPF tracepoint programs: build-priority clamping and audit telemetry.

| | |
|---|---|
| Status | Kernel probes, loader, daemon and clamp implemented (spec 0014). Live gate is operator-run. |
| Roadmap phase | Phase 2 |
| Research | `cerebral-work/research@4068134:agentic-systems/designs/rust-native-agent-substrate.md section 3.3` |
| Spec | `specs/0014-ebpf-priority-clamping-and-audit-probes.md` |
| Depends on | nothing in the workspace yet; `cas-pool` will depend on this crate |

The layering rule is in `specs/0001-cas-architecture-and-crate-layout.md`: a crate
depends only on crates in lower layers.

## Layout

| path | what |
|---|---|
| `ebpf/` | The kernel program. `#![no_std]`, target `bpfel-unknown-none`, outside the workspace. Its ELF declares `Dual MIT/GPL`. |
| `ebpf/cas-ebpf.o`, `ebpf/cas-ebpf.o.sha256` | The committed object and its hash. `build.rs` refuses to compile the crate when they disagree. |
| `src/wire.rs` | The 168-byte `ExecEvent` record, parsed field by field. |
| `src/audit.rs` | `RunHash`: SHA-256 over the records in delivery order, plus the dropped count, into an `AuditReceipt`. |
| `src/clamp.rs` | `PRIO_PGRP` clamping through rustix. `enter_clamped_group` is the `pre_exec` body for `cas-pool`. |
| `src/probe.rs` | Host probe: BTF, `unprivileged_bpf_disabled`, capabilities, tracefs. Tests probe and skip on it. |
| `src/handle.rs` | `ProbeHandle`, the interface the daemon drives, and `FakeProbeHandle` for unprivileged tests. |
| `src/loader.rs` | `Probes`: loads the object, attaches the three `sched` tracepoints, owns the maps. |
| `src/protocol.rs`, `src/daemon.rs`, `src/client.rs` | The `cas-ebpfd` line protocol, the tokio socket server, the blocking client. |
| `src/bin/cas-ebpfd.rs` | The daemon binary. |
| `cas-ebpfd.service` | The systemd unit (root, `CAP_BPF CAP_PERFMON CAP_SYS_NICE`, socket `/run/cas/ebpf.sock`). |

## Decisions (the operator, 2026-10-09)

- **Clean-room Aya implementation:** `aya` 0.14.0 and `aya-ebpf` 0.2.1 (MIT OR Apache-2.0).
  Zero code copied from `neutral-engineering/autonice` (GPL-2.0).
- **Licensing boundary:** the kernel object declares `Dual MIT/GPL` in its `license`
  section. The userspace crate stays proprietary.
- **Scope:** worker process trees only. No host-wide autonice.
- **Clamp method:** `cas-pool` puts each worker in its own process group and calls
  `setpriority(PRIO_PGRP, pgid, 19)` once before `exec`. Measured on ceres:
  `PRIO_PROCESS` clamps one thread only; `PRIO_PGRP` clamps all.
- **Daemon:** a root systemd unit `cas-ebpfd` owns the maps and the ring buffer and
  serves unprivileged `cas-pool` over a Unix socket with a fixed allowlist. Every
  request is checked against the peer's uid (`SO_PEERCRED`): own pids only, a per-uid
  quota, own events only. `--socket-group` names who may connect.
- **Audit:** exec events only (CER-2720 narrowed). `run_hash` is a per-run transcript digest.

## Build and test

```bash
# Workspace side, stable Rust, no privilege needed. The live tests print `skipped`.
cargo nextest run -p cas-ebpf --build-jobs 2

# Rebuild the kernel object (nightly, rust-src, bpf-linker via mise). Commit .o and .sha256.
make ebpf-build

# Live gate on ceres: builds the test binary unprivileged, runs it under sudo. Operator-run.
make ebpf-test
```

Toolchain on ceres (verified 2026-10-09): `mise use -g github:aya-rs/bpf-linker@v0.11.1`
and `rustup component add rust-src --toolchain nightly`. `cargo install bpf-linker` fails
here because it needs system LLVM 23.

## Run the daemon

```bash
cargo build -p cas-ebpf --release -j 2
sudo install -m 0755 target/release/cas-ebpfd /usr/local/bin/cas-ebpfd
sudo install -m 0644 crates/cas-ebpf/cas-ebpfd.service /etc/systemd/system/
sudo systemctl daemon-reload && sudo systemctl enable --now cas-ebpfd
```

Without the daemon, `cas-pool` still clamps with `PRIO_PGRP` and marks the receipt
`unattested` (spec 0014, Deployment).
