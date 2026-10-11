---
status: planned
created: 2026-10-10
tags: [saga, agents, contract]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy, 0025-wave-execution-on-the-custody-chain, 0037-repository-migration-to-uas]
---

# 0046-saga-team-and-topology-files

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

`.saga/teams.toml` and `.saga/topologies.toml` define the `uas-estate` pane gang. Saga's
`bifrost::yggdrasil` reads them when saga runs from this repo. the operator ruled on 2026-10-10
that saga owns all team-checking (Notes), so uas runs no validator of its own (D5). The gap is
that no document states what saga must reject in these files, or what uas guarantees in them.
Spec 0037 D11 only lists the two files as post-cutover work. [reported: research lane w1X:p1,
mesh 5871db01; integrating lane w25:p1] The rename of 2026-10-10 rewrote the persona names in
them with no check that each persona file exists, and the research lane ranked this gap third.

Saga does validate splits and slot coverage. But it falls back to its built-in catalog on an
invalid file, prints one stderr line, and then names the wrong fault (F9). It accepts a persona
that does not exist, a member role that differs from its slot, a misspelt key (F10), and a
harness with no model entry (F11). This spec is the contract: what uas's files guarantee, and
what saga's loader must reject, fail closed.

### Facts

F1 to F13 were measured 2026-10-10 on ceres at `origin/development` 6e7510e (19:03 -0400), in
worktree `/tmp/uas-w46`, since removed. On 2026-10-10 the live files were re-read on branch
`fix/spec-0046-critic` at d4b2e51: `git diff 6e7510e d4b2e51 -- .saga xtask Cargo.toml Cargo.lock
vendor/saga Makefile 'specs/0004-*.md'` prints nothing, F3 and F8 pass again, and F14 ran there.
`.saga` is also unchanged at `origin/development` 7b2703f, of which 6e7510e is an ancestor.

- **F1 [verified]:** `teams.toml` holds one team, `uas-estate`, on topology `estate-cockpit`,
  with nine members. Command: `cat -n .saga/teams.toml`. Labels come from F2.

  | Slot | Label | Role | Harness | Effort | Persona |
  |---|---|---|---|---|---|
  | 0 | lead | brokkr | (default) | medium | uas-foreman |
  | 1 | spec-designer | mimir | agy | (default) | (none) |
  | 2 | architect | mimir | (default) | max | uas-architect |
  | 3 | builder-top | sindri | (default) | medium | uas-builder |
  | 4 | builder-mid | sindri | (default) | medium | uas-builder |
  | 5 | builder-bottom | sindri | (default) | medium | uas-ebpf-builder |
  | 6 | engineer | sindri | (default) | high | uas-engineer |
  | 7 | critic | loki | (default) | high | uas-critic |
  | 8 | prober | thjalfi | (default) | low | uas-prober |

- **F2 [verified]:** `topologies.toml` holds one topology, `estate-cockpit`: nine slots and
  eight splits. Each member role in F1 equals its slot role. The splits, as
  `from direction ratio -> new`, in order: `0 right 0.34 -> 3`, `3 right 0.5 -> 6`,
  `0 down 0.34 -> 1`, `1 down 0.5 -> 2`, `3 down 0.34 -> 4`, `4 down 0.5 -> 5`,
  `6 down 0.34 -> 7`, `7 down 0.5 -> 8`. Each `from` was introduced by an earlier step. Each of
  slots 1 to 8 is introduced once. Command: `cat -n .saga/topologies.toml`.
- **F3 [verified]:** every persona in F1 exists, and each persona file's frontmatter `name`
  equals its stem. Commands: `for p in $(grep -oE 'persona = "[^"]+"' .saga/teams.toml | cut
  -d'"' -f2); do test -f .claude/agents/$p.md && echo "ok $p" || echo "MISSING $p"; done`
  printed eight `ok` lines; `sed -n 's/^name: //p'` on each `.claude/agents/uas-*.md` printed
  its own stem.
- **F4 [verified]:** no code in this repo parses the files. Command: `grep -rnE
  'teams\.toml|topologies\.toml|yggdrasil' crates xtask Makefile .github` exits 1. Control:
  `grep -rnE 'spec_check' xtask` finds `xtask/src/main.rs`. D5 keeps it so.
- **F5 [verified]:** `vendor/saga` has 13 files in `contracts/` and 9 in `flows/`. None defines
  a team or topology type. Commands: `grep -rnE 'struct (Team|Topology|Member|Slot|Split)'
  vendor/saga` and `grep -rliE 'topolog|yggdrasil' vendor/saga` both exit 1.
  [reported, `vendor/saga/README.md`] It was vendored from `unsigned-gg/saga@1e4be86`.
- **F6 [verified]:** the schema is in `cerebral-work/saga@b526308` (branch `development`,
  commit date 2026-10-10), checkout `<home>/projects/platform/runtime/saga`. Command:
  `git -C <checkout> rev-parse --short HEAD`. In `crates/saga-domain/src/yggdrasil.rs`:
  `NorseRole` line 20 (eight kebab-case roles: ratatoskr, mimir, brokkr, sindri, thjalfi,
  huginn, muninn, loki); `SplitDirection` line 774 (right, down); `SlotSpec` line 793 (role,
  label; the slot id is the array position); `SplitStep` line 805 (from, direction, ratio,
  new); `TopologyTemplate` line 815 (slots, splits); `TeamMember` line 1127 (slot, role, and
  optional harness, effort, persona); `TeamRecipe` line 1141 (topology, members).
  `EffortLevel` is in `world_model.rs` line 13 (low, medium, high, max). The loader is
  `crates/bifrost/src/yggdrasil/config.rs`, lines 13 and 15.
- **F7 [verified]:** `TopologyTemplate::validate` (line 927) rejects: no slots, an empty
  label, `splits != slots - 1`, a `from` or `new` out of range, a `from` that no earlier step
  introduced, a ratio outside the open interval (0, 1), and a slot introduced twice.
  `TeamCatalog::resolve` (line 1365) rejects: an unknown topology, a slot out of range, a slot
  assigned twice, and a slot left unassigned. It takes the member's role, not the slot's.
  The `SlotSpec.role` doc (line 794) says the slot role applies when a team does not say
  otherwise. Command: `sed -n 790,1450p` on that file.
- **F8 [verified]:** a member that omits `harness` or `effort` gets it from the built-in role
  table: `let mut roles` at line 1269 and eight inserts, one per role, at lines 1270 to 1277.
  Slot 1 resolved to effort `max`. Command: `saga yggdrasil team show uas-estate` from the repo
  root, exit 0, nine rows; re-run at d4b2e51 with the same result.
- **F9 [verified]:** a malformed or invalid file makes saga use its built-in catalog and print
  one stderr line (`config.rs` lines 43 to 63 and 74 to 94). Probe: in a scratch copy of
  `.saga`, the last split set to `new = 7`. `saga yggdrasil team show uas-estate` printed
  "names topology estate-cockpit, which the catalog lacks", then "unknown team: uas-estate",
  and exited 1. Control: `team show no-such-team` exits 1, `team show uas-estate` exits 0.
- **F10 [verified]:** in scratch copies, `saga yggdrasil team show uas-estate` exited 0 for
  each of: persona `uas-ghost`, which has no file; slot 7 with role `sindri` on a `loki` slot;
  and the key misspelt `persnoa`, where the persona silently vanished from the row. The binary
  is `<home>/.local/bin/saga`, built 2026-10-10 16:29 (`stat`), a link into
  `<home>/projects/unsigned/saga`, which links to the F6 checkout. [asserted] The binary
  behaves as b526308 does; its build commit is unproven. It prints no version (`saga --version`
  is rejected), and HEAD b526308 (18:52) is newer than it. `git diff --stat bc04f25 b526308 --
  <the four schema, loader and CLI files>` prints nothing; that narrows the gap but does not
  close it. Proof: build saga at b526308 into a scratch target directory and re-run F9, F10 and
  F14 with that binary.
- **F11 [verified]:** `teams.toml` line 10 says "Effort selects the model tier (low haiku,
  medium sonnet, high opus, max fable)". That holds for harness `claude` only: `team show`
  printed `gemini-3.8-flash-high` for slot 1 on `agy`. This repo has no `.saga/models.toml`
  (`ls .saga`), so saga's built-in model table applies. The saga doc (line 1117) names the
  harness keys `claude`, `omp`, `opencode` and `agy`. In `crates/volva/src/model_table.rs` the
  built-in table holds those four keys (lines 50 to 84). `.saga/models.toml` overlays it entry
  by entry and can add keys, in lower case (lines 86 to 104). An unknown harness gets an
  invented model, `<harness>-reasoning` at high or max (lines 114 to 121).
- **F12 [verified]:** spec 0004 `### toml` (line 112) pins `toml` at 0.8 (MIT OR Apache-2.0)
  for `uas-engine` and `uas-cli`. The root `Cargo.toml` line 39 has `toml = "0.8"`;
  `Cargo.lock` has 0.8.23. `xtask/Cargo.toml` depends on `anyhow` only. The pin is prose, not a
  `| Crate | Version |` row, so `pin-check` does not bind it: `check` takes pins only from
  `parse_pins`, which reads only rows under that header (`xtask/src/pin_check.rs` lines 126 to
  188). Commands: `grep -n -i toml specs/0004-*.md`, `grep -nE '^(toml|anyhow|serde)\b'
  Cargo.toml`, `grep -nA1 '^name = "toml"$' Cargo.lock`. It backed D9, now dropped.
- **F13 [reported, spec 0025 F9]:** a saga flow step has an optional `persona`.
  [verified] `vendor/saga/contracts/flow.rs` line 64 is `pub persona: Option<String>`
  (`grep -n persona vendor/saga/contracts/flow.rs`).
  No flow here sets one: `grep -rnE '^\s*persona\s*=' .saga/flows vendor/saga/flows` prints
  nothing.
- **F14 [verified]:** on 2026-10-10 at d4b2e51, per Test row but the flow row: copy `.saga` and
  `.claude/agents` into an empty directory, make the Fixture cell's edit, and run `saga yggdrasil
  team show uas-estate` there with the F10 binary. The Today cell is the exit code and whether
  stderr names that fault. `team show` reads no persona file: it copies the name (yggdrasil.rs
  line 1433); the claude launch passes it as `--agent` (bifrost `yggdrasil/topology.rs` 127-128).

## Design

Saga's loader is the sole validator of these files (D5); uas adds no code for them. This
section is the contract. REQ-0046-01 to REQ-0046-03 bind uas's files and hold today (F1 to F3,
F8); REQ-0046-04 to REQ-0046-11 are saga's to implement. To reject is to exit non-zero with no
fallback to the built-in catalog, printing a fault that names the file, table path and key.
Saga names the faults of REQ-0046-04 to 06 but falls back (F9); it accepts those of 07 to 09.

### Decisions

A `ruled` row follows from the ruling (Notes); a `proposed: for the saga lane` row needs saga's
agreement; a row with a uas-file half and a saga half marks each. Filed: in mesh 8decb562 (Plan).

| # | Decision | Rationale | Source |
|---|---|---|---|
| D1 | Saga at `cerebral-work/saga@b526308` owns the schema (F6). | Only saga reads them (F4); `vendor/saga` lacks the types (F5). | w25:p1 brief, 2026-10-10; ruled |
| D2 | Saga rejects a break of any D2 rule; uas files obey them (F1, F2). | Saga runs these checks today (F7); the contract names them. | w25:p1 brief, 2026-10-10; ruled, holds in saga but for D12 (F7, F14) |
| D3 | A persona file's frontmatter `name` equals its stem. | A rename cannot then leave a dangling persona. | w25:p1 brief, 2026-10-10; uas files hold it (F3); saga check proposed: for the saga lane |
| D4 | The comment atop `teams.toml` records the operator's rulings and stays prose. | Its tier mapping fits `claude` only (F11). | w25:p1 brief, 2026-10-10 |
| D5 | Saga's loader is the sole validator; uas runs no team-check. | The ruling moves all team-checking to saga (Notes). | the operator, lane w25:p1, 2026-10-10; ruled |
| D6 | Member role equals slot role. | `topologies.toml` line 6: teams set harness, effort, persona. | w25:p1 brief, 2026-10-10; uas files hold it (F2); saga check proposed: for the saga lane; the operator to rule |
| D7 | A uas team's `topology` names a table in `.saga/topologies.toml`. | A saga built-in can change with no uas commit; saga, unlike team-check, sees them. | uas-engineer, 2026-10-10; uas files only; proposed |
| D8 | A key outside the D8 list fails saga's load. | Saga ignores it, so a misspelt key drops a value (F10). | uas-engineer, 2026-10-10; proposed: for the saga lane (filed) |
| D9 | Dropped: xtask gets no `toml`, as uas adds no code here. | Spec 0045 cites D9; a note there is the lane's follow-up (Notes). | Dropped by the ruling, 2026-10-10 |
| D10 | Dropped: no `make team-check` and no `hygiene` entry. | Saga does the checking (D5). | Dropped by the ruling, 2026-10-10 |
| D11 | A persona is kebab-case. | It cannot then name a path outside `.claude/agents/` (F14). | uas-engineer, 2026-10-10; uas files hold it (F1); saga check proposed: for the saga lane |
| D12 | Saga fails closed: an invalid uas file stops the command and names its real fault. | Saga's fallback misnames faults (F9). | w25:p1 brief, amended for the ruling, 2026-10-10; proposed: for the saga lane (filed) |
| D13 | A table that lacks a key the D8 list requires fails saga's load. | Saga's serde parse rejects it today (F14). | uas-engineer, 2026-10-10; ruled, holds in saga but for D12 (F14) |
| D14 | `harness` must be a saga model-table name (D14 note). | Saga's doc says so (line 1117) but runs any name (F14). | uas-engineer, 2026-10-10; uas files hold it (F1); saga check proposed: for the saga lane |

D9 and D10 keep their numbers, so citations of them still resolve.

D2 list, cited as D2.n. Each rule has a Test row.
- Topology and slots (REQ-0046-04): 1. at least one slot; 2. exactly one split fewer than
  slots; 3. a non-empty `label` per slot; 4. a slot `role` among the eight `NorseRole` names.
- Splits, walked in order (REQ-0046-04): 5. `from` is slot 0 or a slot an earlier split
  introduced; 6. `from` and `new` below the slot count; 7. `new` not already present, slot 0
  included; 8. `direction` is `right` or `down`; 9. `ratio` inside the open interval (0, 1).
- Members (REQ-0046-05): 10. every slot of the topology assigned exactly once; 11. no member
  `slot` at or past the slot count; 12. a set `effort` is `low`, `medium`, `high` or `max`.

D8 list, by table. Top level: `teams` and `roles` in `teams.toml`, `topologies` in
`topologies.toml`. Team: `topology`, `members`, both required. Member: `slot` and `role`, both
required, then `harness`, `effort`, `persona`. Role default (`[roles.<role>]`): `harness`,
`effort`, both required. Topology: `slots`, required, and `splits`. Slot: `role`, `label`, both
required. Split: `from`, `direction`, `ratio`, `new`, all required.

D14 note: a `harness` is `claude`, `omp`, `opencode`, `agy`, or the lower-cased name of a
`[models.<key>]` table in `.saga/models.toml` if it exists; a file that does not parse is a fault.
Saga overlays that file on the four (F11), so it adds names and removes none.

### Requirements (EARS)

- **[REQ-0046-01]** (Ubiquitous): The uas `.saga/teams.toml` and `.saga/topologies.toml` shall
  break no rule of REQ-0046-04 to REQ-0046-10, and `saga yggdrasil team show uas-estate`, run
  from the repo root, shall exit 0 with nine rows (F8).
- **[REQ-0046-02]** (Ubiquitous): The uas `.saga/teams.toml` shall give each member the `role`
  of its topology slot (D6) and name only topologies defined in `.saga/topologies.toml` (D7).
- **[REQ-0046-03]** (Ubiquitous): The uas `.saga/teams.toml` shall name only kebab-case
  personas whose file `.claude/agents/<persona>.md` exists with frontmatter `name` equal to its
  stem (D3, D11), and only harness names in saga's model table (D14).
- **[REQ-0046-04]** (Event-driven, saga): WHEN saga loads a uas topology file that breaks a rule
  in D2.1 to D2.9, saga shall reject it with a fault naming the topology, the slot or split
  index, and the key.
- **[REQ-0046-05]** (Event-driven, saga): WHEN saga loads a uas team file whose members break
  D2.10 to D2.12, or whose `topology` its catalog lacks (F9), saga shall reject it with a fault
  naming the team, the member index and the key.
- **[REQ-0046-06]** (Event-driven, saga): WHEN saga loads a uas team or topology file that lacks
  a key the D8 list requires, or holds a value of the wrong type (a slot index that is not a
  non-negative integer, a ratio that is not a number, a name that is not a string), saga shall
  reject it with a fault naming the key and its table path (D13).
- **[REQ-0046-07]** (Event-driven, saga): WHEN saga loads a uas team or topology file with a key
  outside the D8 list, or a `[roles.<x>]` key that is not a saga role name, saga shall reject it
  with a fault naming the key and its table path (D8).
- **[REQ-0046-08]** (Event-driven, saga): WHEN saga loads a uas team file in which a member's
  `role` differs from its slot's `role` (D6), or a set `harness` is not a D14 name, saga shall
  reject it with a fault naming the member index and the key.
- **[REQ-0046-09]** (Event-driven, saga): WHEN saga loads a uas team file in which a member's
  `persona` does not match `^[a-z0-9]+(-[a-z0-9]+)*$`, has no `.claude/agents/<persona>.md`, or
  names a file whose frontmatter `name` differs from its stem, saga shall reject it with a fault
  naming the slot and persona, and both values on a name mismatch (D3, D11).
- **[REQ-0046-10]** (Optional feature, saga): WHERE a step in `.saga/flows/*.toml` names a
  `persona`, saga shall apply REQ-0046-09 to that persona.
- **[REQ-0046-11]** (Unwanted, saga): IF a uas team, topology or flow file breaks a rule of
  REQ-0046-04 to REQ-0046-10, THEN saga shall exit non-zero with that fault and the file's
  path, and shall not fall back to its built-in catalog (D12, F9).

## Plan

uas adds no code, make target or CHANGELOG line here (D5, D9, D10); the integrator sends mesh.

- [ ] Track the four asks filed with the saga lane, mesh 8decb562 (ruling relayed in c3fa6a36;
      [reported: w25:p1 brief]): fail closed (REQ-0046-11), name the real fault (REQ-0046-04 to
      10), reject unknown keys (REQ-0046-07), and export the schema or a `validate` command (D1).
- [ ] File the saga halves not yet filed: harness names (D14), persona checks (D3, D11,
      REQ-0046-09 and 10), and role equality (D6, REQ-0046-08) once the operator rules on D6.
- [ ] Once saga fails closed, verify the uas files against the contract: build saga at a known
      commit, run every Test row with `saga yggdrasil team show` on a scratch copy, and record
      the commit. Each fixture row must reject; the positive control must exit 0.
- [ ] Write the follow-up migration spec for CPM: specs 0024 and 0025 and the `uas wave`
      commands move to saga under the same ruling. It is out of this spec's scope.

## Test

```bash
d=$(mktemp -d) && mkdir "$d/.claude" && cp -r .saga "$d/" && cp -r .claude/agents "$d/.claude/"
# From the repo root. Make the row's one-line edit under "$d", then:
(cd "$d" && saga yggdrasil team show uas-estate); echo "exit $?"
```

Each row but the last is a fixture saga must reject (Design), made in the scratch copy. The
Fixture cell names one line, as `file:line` and the new text; a bare file name is under
`.saga/`. Indices are 0-based. The Fault cell is the table path saga must name. The Today cell
is the baseline: 23 rows probed on 2026-10-10 with the F10 binary, build commit asserted (F14).
- **accepts**: exit 0. These six rows are red today, which is the point: they are saga's work.
- **falls back**: stderr names the fault, saga loads its built-ins, and `team show` exits 1 on
  the now unknown team (F9). The fault is named, but REQ-0046-11 is red.
- **not probed**: the flow row. `team show` reads no flow, `saga flow` has only `draft`, and
  saga-flow `instantiate.rs` line 219 drops only a blank persona.

| Fixture | Fault saga must name | Today (F14) | Covers |
|---|---|---|---|
| `teams.toml:26` `persona = "uas-ghost"` (primary control) | `teams.uas-estate.members[7].persona` | accepts (F10) | REQ-0046-09, D3 |
| `topologies.toml:8` `topologies.empty = { slots = [] }` | `topologies.empty.slots` | falls back | REQ-0046-04, D2.1 |
| `topologies.toml:30` deleted: 7 splits for 9 slots | `topologies.estate-cockpit.splits` | falls back | REQ-0046-04, D2.2 |
| `topologies.toml:12` `label = ""` | `topologies.estate-cockpit.slots[1].label` | falls back | REQ-0046-04, D2.3 |
| `topologies.toml:12` `role = "odin"` | `topologies.estate-cockpit.slots[1].role` | falls back | REQ-0046-04, D2.4 |
| `topologies.toml:25` `from = 2`, before split 3 adds slot 2 | `topologies.estate-cockpit.splits[2].from` | falls back | REQ-0046-04, D2.5 |
| `topologies.toml:30` `new = 9` | `topologies.estate-cockpit.splits[7].new` | falls back | REQ-0046-04, D2.6 |
| `topologies.toml:30` `new = 7`: slot 7 twice, slot 8 none | `topologies.estate-cockpit.splits[7].new` | falls back (F9) | REQ-0046-04, D2.7 |
| `topologies.toml:30` `direction = "left"` | `topologies.estate-cockpit.splits[7].direction` | falls back | REQ-0046-04, D2.8 |
| `topologies.toml:30` `ratio = 1.0` | `topologies.estate-cockpit.splits[7].ratio` | falls back | REQ-0046-04, D2.9 |
| `topologies.toml:30` `ratio = 0.0` | `topologies.estate-cockpit.splits[7].ratio` | falls back | REQ-0046-04, D2.9 |
| `teams.toml:24` `slot = 4`: slot 4 twice, slot 5 none | `teams.uas-estate.members[5].slot` | falls back | REQ-0046-05, D2.10 |
| `teams.toml:27` `slot = 9` | `teams.uas-estate.members[8].slot` | falls back | REQ-0046-05, D2.11 |
| `teams.toml:17` `topology = "cockpit-grid"`, a saga built-in | `teams.uas-estate.members[6].slot` | falls back on slot 6, right for that topology | REQ-0046-05, D2.11 |
| `teams.toml:26` `role = "sindri"` on a `loki` slot | `teams.uas-estate.members[7].role` | accepts (F10) | REQ-0046-08, D6 |
| `teams.toml:26` `effort = "ultra"` | `teams.uas-estate.members[7].effort` | falls back | REQ-0046-05, D2.12 |
| `teams.toml:20` `harness = "codex"` | `teams.uas-estate.members[1].harness` | accepts, `codex-reasoning` | REQ-0046-08, D14 |
| `teams.toml:26` `persona = "../agents/uas-critic"` | `teams.uas-estate.members[7].persona` | accepts | REQ-0046-09, D11 |
| `.claude/agents/uas-critic.md:2` `name: uas-critic-old` | `name`, with both values | accepts | REQ-0046-09, D3 |
| `teams.toml:26` key `persona` misspelt `persnoa` | `teams.uas-estate.members[7].persnoa` | accepts (F10) | REQ-0046-07, D8 |
| `teams.toml:22` `slot = "3"` | `teams.uas-estate.members[3].slot` | falls back | REQ-0046-06 |
| `topologies.toml:12` key `label` removed | `topologies.estate-cockpit.slots[1].label` | falls back | REQ-0046-06, D13 |
| `flows/uas-builder.toml:23` added: `persona = "uas-ghost"` | `step[0].persona` | not probed | REQ-0046-10 |
| the repo's own files (positive control) | none | exit 0, nine rows (F8) | REQ-0046-01 |

Negative control: `effort = "ultra"` falls back with exit 1, so the probe can go red. Primary
control: the `uas-ghost` row turns from accepts to a rejection when saga ships the persona check.
The `cockpit-grid` row's fault is right for that topology; D7 binds uas's files only. Until saga
ships, `team show` cannot prove REQ-0046-02 or 03; F1 to F3, measured at d4b2e51, are the proof.

## Notes

- The ruling (the operator, lane w25:p1, 2026-10-10 about 23:55Z, verbatim): "migrate all CPM and
  team-checking from uas to saga". [reported: w25:p1 brief, 2026-10-10]
- Follow-up: the CPM half of the ruling (specs 0024 and 0025, the `uas wave` commands) needs
  its own migration spec. It is out of this spec's scope.
- Changed from the pre-ruling draft: the `cargo xtask team-check` validator is gone (D5, D9, D10).
- Changed: each requirement binds either uas's files or saga's loader.
- Changed: the Test rows are saga fixtures run with `saga yggdrasil team show`; F14 is the baseline.
- Spec 0045's plan cites spec 0046 D9 for `toml` in xtask. D9 is dropped, so that item carries
  the spec 0004 `toml` amendment itself; a note in spec 0045 is the lane's follow-up.
- Caveat: the installed saga's build commit is asserted (F10); the Plan's verify step builds one.
- Parked: saga's doc (line 1134) says a persona is for `claude`; the contract allows it elsewhere.
- Parked: `vendor/saga/README.md` names `unsigned-gg/saga@1e4be86`; the live remote is
  `cerebral-work/saga` at b526308 (F5, F6). Re-vendoring is outside this spec.
- The `teams.toml` effort comment (F11) maps tiers for `claude` only. D4 keeps it prose.
