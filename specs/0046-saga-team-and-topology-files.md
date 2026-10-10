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
them, and no spec states their schema. The rename of 2026-10-10 rewrote the persona names in
them, with no check that each persona file exists. Spec 0037 D11 only lists the two files as
post-cutover work. Source of the gap: research lane w1X:p1, mesh 5871db01, ranked third;
integrating lane w25:p1 gap analysis, 2026-10-10.

Saga does validate splits and slot coverage. But it falls back to its built-in catalog on an
invalid file, prints one stderr line, and then names the wrong fault (F9). It accepts a persona
that does not exist, a member role that differs from its slot, and a misspelt key (F10). This
spec adds a pre-flight, `make team-check`, that runs without saga and fails closed.

### Facts

All facts measured 2026-10-10 on ceres, in worktree `/tmp/uas-w46` at `origin/development`
6e7510e, unless the line says otherwise.

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
  table (lines 1269 to 1276), which covers all eight roles. Slot 1 resolved to effort `max`.
  Command: `saga yggdrasil team show uas-estate` in `/tmp/uas-w46`, exit 0, nine rows.
- **F9 [verified]:** a malformed or invalid file makes saga use its built-in catalog and print
  one stderr line (`config.rs` lines 43 to 63 and 74 to 94). Probe: in a scratch copy of
  `.saga`, the last split set to `new = 7`. `saga yggdrasil team show uas-estate` printed
  "names topology estate-cockpit, which the catalog lacks", then "unknown team: uas-estate",
  and exited 1. Control: `team show no-such-team` exits 1, `team show uas-estate` exits 0.
- **F10 [verified]:** in scratch copies, `saga yggdrasil team show uas-estate` exited 0 for
  each of: persona `uas-ghost`, which has no file; slot 7 with role `sindri` on a `loki` slot;
  and the key misspelt `persnoa`, where the persona silently vanished from the row. The binary
  is `~/.local/bin/saga`, a link into `/home/ctodie/projects/unsigned/saga`, built 2026-10-10
  16:29 (`stat`). Its build commit is unproven. That checkout's HEAD is b526308 (18:52), newer
  than the binary. The four schema, loader and CLI files are unchanged from bc04f25 (16:29) to
  b526308: `git diff --stat bc04f25 b526308 -- <the four files>` prints nothing.
- **F11 [verified]:** `teams.toml` line 10 says "Effort selects the model tier (low haiku,
  medium sonnet, high opus, max fable)". That holds for harness `claude` only: `team show`
  printed `gemini-3.8-flash-high` for slot 1 on `agy`. This repo has no `.saga/models.toml`
  (`ls .saga`), so saga's built-in model table applies. The saga doc (line 1117) names the
  harness keys `claude`, `omp`, `opencode` and `agy`.
- **F12 [verified]:** spec 0004 `### toml` (line 112) pins `toml` at 0.8 (MIT OR Apache-2.0)
  for `uas-engine` and `uas-cli`. The root `Cargo.toml` line 39 has `toml = "0.8"`;
  `Cargo.lock` has 0.8.23. `xtask/Cargo.toml` depends on `anyhow` only. The pin is prose, not a
  `| Crate | Version |` row, so `pin-check` does not bind it (`xtask/src/pin_check.rs` line 159).
  Commands: `grep -n -i toml specs/0004-*.md`, `grep -nE '^(toml|anyhow|serde)\b' Cargo.toml`,
  `grep -nA1 '^name = "toml"$' Cargo.lock`.
- **F13 [reported, spec 0025 F9]:** a saga flow step has an optional `persona`.
  [verified] `vendor/saga/contracts/flow.rs` line 64 is `pub persona: Option<String>`
  (`grep -n persona vendor/saga/contracts/flow.rs`).
  No flow here sets one: `grep -rnE '^\s*persona\s*=' .saga/flows vendor/saga/flows` prints
  nothing.

## Design

`cargo xtask team-check` reads both files as `toml::Table` and walks each table against an
allow-list of keys. It needs no serde derive and no saga binary. It enforces every check in F7
and every key and type saga's serde parse demands, so it does not pass a file saga rejects. It also enforces the checks
saga lacks (F10). It reports every fault with the file, the table path and the key.

### Decisions

| # | Decision | Source |
|---|---|---|
| D1 | Saga's yggdrasil types at `cerebral-work/saga@b526308` own the schema (F6). This repo vendors nothing new. This spec states the invariants the two files must hold. | Lane w25:p1 brief, 2026-10-10 |
| D2 | A `make team-check` target runs `cargo xtask team-check` and joins `make hygiene`. It holds the brief's checks plus saga's own (F7): split count, ordered introduction, ratio range, direction, labels, slot coverage, role names. | Lane w25:p1 brief, 2026-10-10; widened to F7 by uas-engineer, 2026-10-10 |
| D3 | A persona file's frontmatter `name` equals its file stem, so a rename cannot leave a dangling persona. A persona is kebab-case, so it cannot name a path outside `.claude/agents/`. | Lane w25:p1 brief, 2026-10-10; kebab-case rule uas-engineer, 2026-10-10 |
| D4 | The comment block at the top of `teams.toml` records Christian's rulings. It is prose and is not parsed. F11 shows its model mapping holds for `claude` only; no check reads it. | Lane w25:p1 brief, 2026-10-10 |
| D5 | Saga is the authority at runtime for what saga checks. team-check is the pre-flight for those checks and the only check for persona files, role equality and unknown keys (F10). Acceptance is team-check plus, where saga is installed, `saga yggdrasil team show uas-estate` exit 0. Proposed: the brief made `team show` the sole acceptance; F10 shows it passes three faults. | Lane w25:p1 brief, amended by uas-engineer, 2026-10-10; Christian to rule |
| D6 | A member's role equals its topology slot's role. This is stricter than saga (F7, F10). It follows the ruling in `topologies.toml` line 6: "the team file sets harness, effort and persona". | Lane w25:p1 brief, 2026-10-10; proposed, Christian to rule |
| D7 | A team's `topology` names a table in this repo's `topologies.toml`. team-check cannot see saga's six built-in archetypes. A team that wants one copies it into the file. | uas-engineer, 2026-10-10; proposed |
| D8 | A key outside the saga schema fails the check. Saga ignores it (F10). Top-level keys: `teams` and `roles` in `teams.toml`, `topologies` in `topologies.toml`. Member keys: `slot`, `role`, `harness`, `effort`, `persona`. Team keys: `topology`, `members`. Topology keys: `slots`, `splits`. Slot keys: `role`, `label`. Split keys: `from`, `direction`, `ratio`, `new`. Role-default keys: `harness`, `effort`. Required: member `slot` and `role`; team `topology` and `members`; topology `slots`; slot `role` and `label`; all four split keys; both role-default keys. | uas-engineer, 2026-10-10; proposed |
| D9 | xtask takes `toml = { workspace = true }`. Spec 0004 `### toml` first names `xtask` as a user (rule 12). No new crate and no version change. | AGENTS.md rule 12; uas-engineer, 2026-10-10 |

### Requirements (EARS)

- **[REQ-0046-01]** (Ubiquitous): `cargo xtask team-check` shall parse `.saga/teams.toml` and
  `.saga/topologies.toml` with the pinned `toml` crate, run without a saga binary, and exit
  non-zero on any fault in REQ-0046-02 to REQ-0046-10, naming the file, table path and key.
  `make team-check` shall run it, and `make hygiene` shall include `team-check`.
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
  or `effort`, team-check shall require `harness` in {claude, agy, omp, opencode} and `effort`
  in {low, medium, high, max}; a member that omits either shall pass.
- **[REQ-0046-07]** (Unwanted): IF a member names a `persona` that does not match
  `^[a-z0-9]+(-[a-z0-9]+)*$` or has no file `.claude/agents/<persona>.md`, THEN team-check
  shall fail and name the slot and persona.
- **[REQ-0046-08]** (Unwanted): IF a named persona file's frontmatter `name` differs from its
  file stem, THEN team-check shall fail and name the file and both values.
- **[REQ-0046-09]** (Unwanted): IF any table in either file carries a key outside the D8
  allow-list, lacks a D8 required key, holds a value of the wrong type (a slot index that is
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
      parses with `toml::Table`, walks the D8 allow-lists, and implements REQ-0046-02 to
      REQ-0046-10. It collects every fault before it exits.
- [ ] Wire `Some("team-check")` and the usage line in `xtask/src/main.rs`.
- [ ] Add a `team-check` target to `Makefile`, add it to `.PHONY`, and add it to the
      `hygiene` prerequisites. Update the `make hygiene` row in `AGENTS.md`.
- [ ] Fixtures: each unit test writes a temp root, as `spec_check.rs` does with
      `std::env::temp_dir()`. It copies the real `.saga` files and the named persona files,
      then changes one line. No `.claude/` tree is committed under `xtask/`, because spec 0025
      D17 halts a wave on a path under `.claude/`.
- [ ] Add one test that runs `check` on the repo root and expects zero faults.
- [ ] Add one `CHANGELOG.md` line under `## [Unreleased]`.

## Test

```bash
cargo test -p xtask -j 2 team_check && make team-check JOBS=2
```

Saga is not in the fenced command, because CI has no saga. Where saga is installed, also run
`saga yggdrasil team show uas-estate` from the repo root; it exits 0 with nine rows (D5).

Each case is a fixture with one changed line. Each must exist, and each must go red when its
own check is deleted. "Saga catches" says whether `saga yggdrasil team show` exits non-zero on
the same fault.

| Case | Saga catches | Requirement |
|---|---|---|
| member persona `uas-ghost`, no such file (primary control) | no, F10 | REQ-0046-07 |
| persona `../agents/uas-critic` | no, not probed; saga reads no persona file | REQ-0046-07 |
| persona file `name: uas-critic-old` under stem `uas-critic` | no, not probed; saga reads no persona file | REQ-0046-08 |
| last split `new = 7`: slot 7 twice, slot 8 orphaned | yes, F9 | REQ-0046-03 |
| split `ratio = 1.0` | yes, F7 (code, not probed) | REQ-0046-03 |
| two members on slot 4, none on slot 5 | yes, F7 (code, not probed) | REQ-0046-04 |
| team `topology = "cockpit-grid"`, not in the file | no, saga has it built in | REQ-0046-04 |
| slot 7 member role `sindri` on a `loki` slot | no, F10 | REQ-0046-05 |
| member `effort = "ultra"` | yes, serde enum (not probed) | REQ-0046-06 |
| member key `persnoa` | no, F10 | REQ-0046-09 |
| member `slot = "3"` | yes, serde (not probed) | REQ-0046-09 |
| topology slot with no `label` | yes, serde (not probed) | REQ-0046-09 |
| flow step `persona = "uas-ghost"` | not probed | REQ-0046-10 |
| the repo's own files (positive control) | exit 0, F8 | REQ-0046-01 |

The primary negative control: delete the file-exists check in `team_check.rs`, and the
`uas-ghost` case must go red. Saga passes that case, so only team-check guards it.

## Notes

- Parked for the saga lane: fail closed on an invalid overlay instead of falling back (F9),
  name the real fault, reject unknown keys (F10), and export the schema or a `validate`
  command. When saga ships these, D2 shrinks to the uas-local checks.
- Parked: saga's doc (line 1134) says a persona is for the `claude` harness. team-check does
  not reject a persona on another harness.
- Parked: `vendor/saga/README.md` names `unsigned-gg/saga@1e4be86`; the live remote is
  `cerebral-work/saga` at b526308 (F5, F6). Re-vendoring is outside this spec.
- The `teams.toml` effort comment (F11) maps tiers for `claude` only. D4 keeps it prose.
- D5, D6 and D7 are proposed. D5 and D6 need Christian's ruling.
