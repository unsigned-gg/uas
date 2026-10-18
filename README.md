# uas

UAS is the Unsigned Agent Substrate. It is a Rust-native, pooled runtime for agent
harnesses. The design calls for durable workflows, kernel confinement, prefix-aware
model routing and an operator dashboard.

**Status:** early runtime. The pool, sandbox, seats, build and operator crates run; the `uas` CLI has `seats`, `build` and wave commands. Version 0.1.0. Public. Licensed
`Apache-2.0 OR MIT` (see `LICENSE-APACHE`, `LICENSE-MIT` and `NOTICE`). The design is a draft from `cerebral-work/research@4068134`.

**Support:** This project is pre-1.0.
It makes no stability or support promise, and any interface may change without notice.
Example code is not for production.

## Crates

| Crate | Role | Phase |
|---|---|---|
| `uas-domain` | Slots, leases, envelopes, attestations. Zero I/O. | 1 |
| `uas-engine` | Durable workflows and saga compensators. | 1 |
| `uas-sandbox` | Landlock rulesets and namespaces. | 2 |
| `uas-ebpf` | eBPF priority clamping and audit probes. | 2 |
| `uas-pool` | Worker slots, warm worktrees, cgroup suspend. | 3 |
| `uas-acp` | JSON-RPC server for IDEs. | 3 |
| `uas-router` | Prefix-hash routing for model calls. | 4 |
| `uas-deck` | Operator dashboard. | 5 |
| `uas-operator` | Kubernetes operator and node agent for `WorkerSlot`. | n/a |
| `uas-storage` | Storage clients. No phase yet. | open |
| `uas-cli` | Binary `uas`. | n/a |

A crate depends only on crates in lower layers. `uas-domain` depends on no other crate
and does no I/O. The full rule and the open gaps are in
[specs/0001](specs/0001-cas-architecture-and-crate-layout.md).

## Relations

- **soma** classifies tasks. **swarm-control** leases slots.
- **saga** validates execution receipts and compensates failures.
- **reverie** stores run memory. The **cortex** mesh carries messages and settle cards.
- UAS integrates with these as services. It does not vendor them.
- `unsigned-gg/substrate` is a different system. It is a Go Kubernetes sandbox runtime.
  UAS does not use it.

## Build

```bash
make help
make check
```

`make check` runs fmt, clippy, tests, doc tests and cargo-deny. Builds use `-j 2` on
shared hosts.

## Where things are

| Need | Go to |
|---|---|
| The rules for people and agents | [AGENTS.md](AGENTS.md) |
| The specs and their registry | [specs/README.md](specs/README.md) |
| Docs | [docs/README.md](docs/README.md) |
| How to contribute | [CONTRIBUTING.md](CONTRIBUTING.md) |
