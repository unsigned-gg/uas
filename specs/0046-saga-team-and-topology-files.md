---
status: planned
created: 2026-10-10
tags: [saga, agents, hygiene, xtask, toml]
priority: high
depends_on: [0002-verification-gates-and-evidence, 0024-estate-saga-cpm-and-agent-sprint-wave-strategy, 0025-wave-execution-on-the-custody-chain, 0037-repository-migration-to-uas]
---

# 0046-saga-team-and-topology-files

> **Status**: planned, **Priority**: high, **Created**: 2026-10-10

## Overview

`.saga/teams.toml` and `.saga/topologies.toml` define the `uas-estate` pane gang. Saga's
`bifrost::yggdrasil` reads them when saga runs from this repo. Nothing in this repo validates
them (F4), and no spec states their schema. Spec 0037 D11 only lists the two files as
post-cutover work. [reported: research lane w1X:p1, mesh 5871db01; integrating lane w25:p1] The
rename of 2026-10-10 rewrote the persona names in them with no check that each persona file
exists, and the research lane ranked this gap third.

Saga does validate splits and slot coverage. But it falls back to its built-in catalog on an
invalid file, prints one stderr line, and then names the wrong fault (F9). It accepts a persona
that does not exist, a member role that differs from its slot, a misspelt key (F10), and a
harness with no model entry (F11). This spec adds a pre-flight, `make team-check`, that runs
without saga and fails closed.

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
  `grep -rnE 'spec_check' xtask` finds `xtask/src/main.rs`.
- **F5 [verified]:** `vendor/saga` has 13 files in `contracts/` and 9 in `flows/`. None defines
  a team or topology type. Commands: `grep -rnE 'struct (Team|Topology|Member|Slot|Split)'
  vendor/saga` and `grep -rliE 'topolog|yggdrasil' vendor/saga` both exit 1.
  [reported, `vendor/saga/README.md`] It was vendored from `unsigned-gg/saga@1e4be86`.
- **F6 [verified]:** the schema is in `cerebral-work/saga@b526308` (branch `development`,
  commit date 2026-10-10), checkout `/home/ctodie/projects/platform/runtime/saga`. Command:
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
  is `~/.local/bin/saga`, built 2026-10-10 16:29 (`stat`), a link into
  `/home/ctodie/projects/unsigned/saga`, which links to the F6 checkout. [asserted] The binary
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
  Cargo.toml`, `grep -nA1 '^name = "toml"$' Cargo.lock`.
- **F13 [reported, spec 0025 F9]:** a saga flow step has an optional `persona`.
  [verified] `vendor/saga/contracts/flow.rs` line 64 is `pub persona: Option<String>`
  (`grep -n persona vendor/saga/contracts/flow.rs`).
  No flow here sets one: `grep -rnE '^\s*persona\s*=' .saga/flows vendor/saga/flows` prints
  nothing.
- **F14 [verified]:** on 2026-10-10 at d4b2e51, per Test row but the flow row: copy `.saga` and
  `.claude/agents` into an empty directory, make the Fixture cell's edit, and run `saga yggdrasil
  team show uas-estate` there with the F10 binary. The Saga cell is the exit code and whether
  stderr names that fault. `team show` reads no persona file: it copies the name (yggdrasil.rs
  line 1433); the claude launch passes it as `--agent` (bifrost `yggdrasil/topology.rs` 127-128).

## Design

`cargo xtask team-check` reads both files as `toml::Table` and walks each table against an
allow-list of keys. It needs no serde derive and no saga binary. It enforces every check in F7
and every key and type saga's serde parse demands, so it passes no file saga rejects. It also
enforces the checks saga lacks (F10, F11). It reports every fault with the file, the table path
and the key.

### Decisions

| # | Decision | Source |
|---|---|---|
| D1 | Saga's yggdrasil types at `cerebral-work/saga@b526308` own the schema (F6); this repo vendors no copy. | Lane w25:p1 brief, 2026-10-10 |
| D2 | team-check enforces saga's own topology and team checks (F7): the D2 list below. | Lane w25:p1 brief; widened to F7 by uas-engineer, 2026-10-10 |
| D3 | A persona file's frontmatter `name` equals its stem, so a rename cannot leave a dangling persona. | Lane w25:p1 brief, 2026-10-10 |
| D4 | The comment block atop `teams.toml` records Christian's rulings and stays prose; no check reads it (F11). | Lane w25:p1 brief, 2026-10-10 |
| D5 | Acceptance: team-check, plus `team show` exit 0 where saga runs. | Brief: `team show` only; uas-engineer, 2026-10-10 (F10); Christian to rule |
| D6 | Member role equals slot role (`topologies.toml` line 6), stricter than saga (F10). | Lane w25:p1 brief, 2026-10-10; proposed, Christian to rule |
| D7 | A team's `topology` names a table in this repo's `topologies.toml`; to use a saga built-in, copy it in. | uas-engineer, 2026-10-10; proposed |
| D8 | A key outside the D8 list below fails the check; saga ignores it (F10). | uas-engineer, 2026-10-10; proposed |
| D9 | xtask takes `toml = { workspace = true }`, no new crate or version; spec 0004 `### toml` names `xtask`. | AGENTS.md rule 12; uas-engineer, 2026-10-10 |
| D10 | `make team-check` runs `cargo xtask team-check` and joins `make hygiene`. | Lane w25:p1 brief, 2026-10-10 |
| D11 | A persona is kebab-case, so it cannot name a path outside `.claude/agents/`. | uas-engineer, 2026-10-10 |
| D12 | Saga rules at runtime on its checks; team-check pre-flights them and alone checks the F10 faults. | Brief, amended by uas-engineer, 2026-10-10 |
| D13 | A table that lacks a key the D8 list requires fails, as saga's serde parse does (F14). | uas-engineer, 2026-10-10; proposed |
| D14 | A `harness` is a saga model-table key: the D14 note below. | uas-engineer, 2026-10-10; read from the file, as saga does (F11); proposed |

D10 to D13 were split from D2, D3, D5 and D8, in order, in critic round one; D14 is new.

D2 list, cited as D2.n. Each rule has a Test row.
1. A topology has at least one slot (REQ-0046-02).
2. A topology has exactly one split fewer than slots (REQ-0046-02).
3. Each slot `label` is non-empty (REQ-0046-02).
4. Each slot `role` is one of the eight `NorseRole` names (REQ-0046-02).
5. Each split's `from` is slot 0 or a slot an earlier split introduced (REQ-0046-03).
6. Each split's `from` and `new` are below the slot count (REQ-0046-03).
7. No split's `new` is already present, slot 0 included (REQ-0046-03).
8. Each split's `direction` is `right` or `down` (REQ-0046-03).
9. Each split's `ratio` is inside the open interval (0, 1) (REQ-0046-03).
10. A team's members assign every slot of its topology exactly once (REQ-0046-04).
11. No member `slot` is at or past the slot count (REQ-0046-04).
12. A set `effort` is `low`, `medium`, `high` or `max` (REQ-0046-06).

D8 list, by table. Top level: `teams` and `roles` in `teams.toml`, `topologies` in
`topologies.toml`. Team: `topology`, `members`, both required. Member: `slot` and `role`, both
required, then `harness`, `effort`, `persona`. Role default (`[roles.<role>]`): `harness`,
`effort`, both required. Topology: `slots`, required, and `splits`. Slot: `role`, `label`, both
required. Split: `from`, `direction`, `ratio`, `new`, all required.

D14 note: a `harness` is `claude`, `omp`, `opencode`, `agy`, or the lower-cased name of a
`[models.<key>]` table in `.saga/models.toml` if it exists; a file that does not parse is a fault.
Saga overlays that file on the four (F11), so it adds names and removes none. The set is read, not
fixed, because saga's doc calls a harness a model-table key (line 1117). Saga runs any name (F14).

### Requirements (EARS)

- **[REQ-0046-01]** (Ubiquitous): `cargo xtask team-check` shall parse `.saga/teams.toml` and
  `.saga/topologies.toml` with the pinned `toml` crate, run without a saga binary, and exit
  non-zero on any fault in REQ-0046-02 to REQ-0046-10. It shall print one line per fault that
  starts with the file and the table path, ending in the key. `make team-check` shall run it,
  and `make hygiene` shall include `team-check`.
- **[REQ-0046-02]** (Ubiquitous): team-check shall require each topology to have at least one
  slot, a non-empty `label` per slot, a slot `role` from the eight saga role names, and exactly
  one split fewer than its slots.
- **[REQ-0046-03]** (Event-driven): WHEN team-check walks a topology's splits in order, it
  shall reject a split whose `from` no earlier step introduced, whose `from` or `new` is not
  below the slot count, whose `new` is already present (slot 0 included), whose `direction` is
  not `right` or `down`, or whose `ratio` is outside the open interval (0, 1).
- **[REQ-0046-04]** (Ubiquitous): team-check shall require each team's `topology` to name a
  table in `.saga/topologies.toml`, and its members to assign every slot of that topology
  exactly once, with no member `slot` at or past the slot count.
- **[REQ-0046-05]** (Ubiquitous): team-check shall require each member's `role` to equal the
  `role` of its topology slot.
- **[REQ-0046-06]** (State-driven): WHILE a member or a `[roles.<role>]` table sets `harness`
  or `effort`, team-check shall require `harness` to be a D14 name and `effort` in {low,
  medium, high, max}; a member that omits either shall pass.
- **[REQ-0046-07]** (Unwanted): IF a member names a `persona` that does not match
  `^[a-z0-9]+(-[a-z0-9]+)*$` or has no file `.claude/agents/<persona>.md`, THEN team-check
  shall fail and name the slot and persona.
- **[REQ-0046-08]** (Unwanted): IF a named persona file's frontmatter `name` differs from its
  file stem, THEN team-check shall fail and name the file and both values.
- **[REQ-0046-09]** (Unwanted): IF any table in either file carries a key outside the D8 list,
  lacks a key the D8 list requires, holds a value of the wrong type (a slot index that is
  not a non-negative integer, a ratio that is not a number, a name that is not a string), or
  has a `[roles.<x>]` key that is not a saga role name, THEN team-check shall fail and name the
  key and its table path.
- **[REQ-0046-10]** (Optional feature): WHERE a step in `.saga/flows/*.toml` names a
  `persona`, team-check shall apply REQ-0046-07 and REQ-0046-08 to that persona.

## Plan

- [ ] Amend spec 0004 `### toml` to name `xtask` as a user (D9). This goes first, in the
      same PR.
- [ ] Add `toml = { workspace = true }` to `xtask/Cargo.toml`.
- [ ] Add `xtask/src/team_check.rs`: `run(root) -> ExitCode` and `check(root) -> Report`. It
      parses with `toml::Table`, walks the D8 list, and implements REQ-0046-02 to
      REQ-0046-10. It reads `.saga/models.toml` when present (D14). It collects every fault
      before it exits.
- [ ] Wire `Some("team-check")` and the usage line in `xtask/src/main.rs`.
- [ ] Add a `team-check` target to `Makefile`, add it to `.PHONY`, and add it to the
      `hygiene` prerequisites. Update the `make hygiene` row in `AGENTS.md`.
- [ ] Fixtures: each unit test writes a temp root, as `spec_check.rs` does with
      `std::env::temp_dir()`. It copies the real `.saga` files, flows included, and the named
      persona files, then makes the one-line edit its Test row names. No `.claude/` tree is
      committed under `xtask/`, because spec 0025 D17 halts a wave on a path under `.claude/`.
- [ ] Add one test that runs `check` on the repo root and expects zero faults.
- [ ] Add one `CHANGELOG.md` line under `## [Unreleased]`.

## Test

```bash
cargo test -p xtask -j 2 team_check && make team-check JOBS=2
```

Saga is not in the fenced command, because CI has no saga. Where saga is installed, also run
`saga yggdrasil team show uas-estate` from the repo root; it exits 0 with nine rows (D5).

Each fixture adds, changes or deletes the one line its Fixture cell names, as `file:line` and
the new text; a bare file name is under `.saga/`. Each must exist, and each must go red when its
own check is deleted. The Fault cell is the expected fault line: the fixture's file, then that
table path (REQ-0046-01). Indices are 0-based array positions. The test asserts that one fault
line starts with them. Of the 24 rows, 23 were probed with saga on 2026-10-10 (F14), with a
binary whose build commit is asserted (F10). The flow row is not probed: `team show` reads no
flow, `saga flow` has only `draft`, and saga-flow `instantiate.rs` line 219 drops only a blank
persona.

| Fixture | Fault | Saga (F14) | Covers |
|---|---|---|---|
| `teams.toml:26` `persona = "uas-ghost"` (primary control) | `teams.uas-estate.members[7].persona` | no, exit 0 (F10) | REQ-0046-07 |
| `topologies.toml:8` `topologies.empty = { slots = [] }` | `topologies.empty.slots` | yes, exit 1 | REQ-0046-02, D2.1 |
| `topologies.toml:30` deleted: 7 splits for 9 slots | `topologies.estate-cockpit.splits` | yes, exit 1 | REQ-0046-02, D2.2 |
| `topologies.toml:12` `label = ""` | `topologies.estate-cockpit.slots[1].label` | yes, exit 1 | REQ-0046-02, D2.3 |
| `topologies.toml:12` `role = "odin"` | `topologies.estate-cockpit.slots[1].role` | yes, exit 1 | REQ-0046-02, D2.4 |
| `topologies.toml:25` `from = 2`, before split 3 adds slot 2 | `topologies.estate-cockpit.splits[2].from` | yes, exit 1 | REQ-0046-03, D2.5 |
| `topologies.toml:30` `new = 9` | `topologies.estate-cockpit.splits[7].new` | yes, exit 1 | REQ-0046-03, D2.6 |
| `topologies.toml:30` `new = 7`: slot 7 twice, slot 8 none | `topologies.estate-cockpit.splits[7].new` | yes, F9 | REQ-0046-03, D2.7 |
| `topologies.toml:30` `direction = "left"` | `topologies.estate-cockpit.splits[7].direction` | yes, exit 1 | REQ-0046-03, D2.8 |
| `topologies.toml:30` `ratio = 1.0` | `topologies.estate-cockpit.splits[7].ratio` | yes, exit 1 | REQ-0046-03, D2.9 |
| `topologies.toml:30` `ratio = 0.0` | `topologies.estate-cockpit.splits[7].ratio` | yes, exit 1 | REQ-0046-03, D2.9 |
| `teams.toml:24` `slot = 4`: slot 4 twice, slot 5 none | `teams.uas-estate.members[5].slot` | yes, exit 1 | REQ-0046-04, D2.10 |
| `teams.toml:27` `slot = 9` | `teams.uas-estate.members[8].slot` | yes, exit 1 | REQ-0046-04, D2.11 |
| `teams.toml:17` `topology = "cockpit-grid"`, a saga built-in | `teams.uas-estate.topology` | no; exit 1 on slot 6 instead | REQ-0046-04, D7 (proposed) |
| `teams.toml:26` `role = "sindri"` on a `loki` slot | `teams.uas-estate.members[7].role` | no, exit 0 (F10) | REQ-0046-05, D6 (proposed) |
| `teams.toml:26` `effort = "ultra"` | `teams.uas-estate.members[7].effort` | yes, exit 1 | REQ-0046-06, D2.12 |
| `teams.toml:20` `harness = "codex"` | `teams.uas-estate.members[1].harness` | no, exit 0, `codex-reasoning` | REQ-0046-06, D14 (proposed) |
| `teams.toml:26` `persona = "../agents/uas-critic"` | `teams.uas-estate.members[7].persona` | no, exit 0 | REQ-0046-07, D11 |
| `.claude/agents/uas-critic.md:2` `name: uas-critic-old` | `name` | no, exit 0 | REQ-0046-08, D3 |
| `teams.toml:26` key `persona` misspelt `persnoa` | `teams.uas-estate.members[7].persnoa` | no, exit 0 (F10) | REQ-0046-09, D8 |
| `teams.toml:22` `slot = "3"` | `teams.uas-estate.members[3].slot` | yes, exit 1 | REQ-0046-09 |
| `topologies.toml:12` key `label` removed | `topologies.estate-cockpit.slots[1].label` | yes, exit 1 | REQ-0046-09, D13 |
| `flows/uas-builder.toml:23` added: `persona = "uas-ghost"` | `step[0].persona` | not probed | REQ-0046-10 |
| the repo's own files (positive control) | none | exit 0, F8 | REQ-0046-01 |

The primary negative control: delete the file-exists check in `team_check.rs`, and the
`uas-ghost` case must go red; saga passes it. The flow row is REQ-0046-10's control: no flow sets
a persona today (F13), so only its added line can make that walk go red.

## Notes

- Parked for the saga lane: fail closed on an invalid overlay instead of falling back (F9),
  name the real fault, reject unknown keys (F10) and unknown harness names (F11), and export
  the schema or a `validate` command. When saga ships these, D2 shrinks to the uas-local checks.
- Parked: saga's doc (line 1134) says a persona is for the `claude` harness. team-check does
  not reject a persona on another harness.
- Parked: `vendor/saga/README.md` names `unsigned-gg/saga@1e4be86`; the live remote is
  `cerebral-work/saga` at b526308 (F5, F6). Re-vendoring is outside this spec.
- The `teams.toml` effort comment (F11) maps tiers for `claude` only. D4 keeps it prose.
- D5 to D8, D13 and D14 are proposed. D5 and D6 need Christian's ruling.
