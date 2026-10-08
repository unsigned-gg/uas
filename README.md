# cas-substrate

CAS is the Cerebral Agent Substrate. It is a Rust-native, pooled runtime for agent
harnesses. The design calls for durable workflows, kernel confinement, prefix-aware
model routing and an operator dashboard.

**Status:** scaffold and domain; no runtime yet. Version 0.1.0. Private. Proprietary
licence (see `LICENSE`). The design is a draft from `cerebral-work/research@4068134`.

## Crates

| Crate | Role | Phase |
|---|---|---|
| `cas-domain` | Slots, leases, envelopes, attestations. Zero I/O. | 1 |
| `cas-engine` | Durable workflows and saga compensators. | 1 |
| `cas-sandbox` | Landlock rulesets and namespaces. | 2 |
| `cas-ebpf` | eBPF priority clamping and audit probes. | 2 |
| `cas-pool` | Worker slots, warm worktrees, cgroup suspend. | 3 |
| `cas-acp` | JSON-RPC server for IDEs. | 3 |
| `cas-router` | Prefix-hash routing for model calls. | 4 |
| `cas-deck` | Operator dashboard. | 5 |
| `cas-storage` | Storage clients. No phase yet. | open |
| `cas-cli` | Binary `cas`. | n/a |

A crate depends only on crates in lower layers. `cas-domain` depends on no other crate
and does no I/O. The full rule and the open gaps are in
[specs/0001](specs/0001-cas-architecture-and-crate-layout.md).

## Relations

- **soma** classifies tasks. **swarm-control** leases slots.
- **saga** validates execution receipts and compensates failures.
- **reverie** stores run memory. The **cortex** mesh carries messages and settle cards.
- CAS integrates with these as services. It does not vendor them.
- `unsigned-gg/substrate` is a different system. It is a Go Kubernetes sandbox runtime.
  CAS does not use it.

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
